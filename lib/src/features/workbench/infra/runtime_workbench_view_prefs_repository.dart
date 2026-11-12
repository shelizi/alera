import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:logging/logging.dart';

final Logger _log = Logger('RuntimeWorkbenchViewPrefsRepository');

class RuntimeWorkbenchViewPrefsRepository({
  required final RuntimeHostClient client,
  required final WorkbenchViewPrefsRepository legacyRepository,
  final Future<void> Function()? beforeAccess,
}) implements WorkbenchViewPrefsRepository {
  int? _revision;
  Map<String, bool>? _lastExplorerShared;
  Map<String, List<String>>? _lastGitFavoritesShared;
  Map<String, Set<String>>? _lastGitFavoritesView;

  @override
  Stream<WorkbenchViewPrefs> get changes => client.runtimeEvents
      .where(
        (event) =>
            event.name == 'workbenchViewPrefsChanged' ||
            event.name == 'runtimeSettingsChanged' ||
            event.name == 'gitFavoritesChanged',
      )
      .asyncMap((_) => load());

  @override
  Future<WorkbenchViewPrefs> load() async {
    final local = await legacyRepository.load();
    try {
      await beforeAccess?.call();
      final record = _asMap(
        await client.runtimeRequest('workbenchViewPrefs.get'),
      );
      _revision = (record['revision'] as num?)?.toInt() ?? 0;
      if (record['desktopInitialized'] != true) {
        await _writeShared(local);
        final merged = await _loadGitFavoritesShared(
          await _loadExplorerShared(local),
        );
        await legacyRepository.save(merged);
        return await _forRuntime(merged);
      }
      final merged = await _loadGitFavoritesShared(
        await _loadExplorerShared(_mergeShared(local, _asMap(record['prefs']))),
      );
      await legacyRepository.save(merged);
      return await _forRuntime(merged);
    } catch (error, stackTrace) {
      // The local prefs still render, so the only visible symptom is that a
      // change made on another device never arrives.
      _log.warning(
        'could not merge shared view prefs from the runtime; using local only',
        error,
        stackTrace,
      );
      _lastExplorerShared = null;
      _lastGitFavoritesShared = null;
      _lastGitFavoritesView = _snapshotFavoriteView(
        local.favoriteBranchesByRepository,
      );
      return _forRuntime(local);
    }
  }

  /// One-time Classic Drift -> Rust migration; thereafter Host values win.
  /// This never shares unrelated Classic layout or panel sizing preferences.
  Future<WorkbenchViewPrefs> _loadExplorerShared(
    WorkbenchViewPrefs prefs,
  ) async {
    try {
      final response = _asMap(
        await client.runtimeRequest(
          'desktopSettings.seedLocal',
          <String, Object?>{'values': _explorerSharedValues(prefs)},
        ),
      );
      final values = _asMap(response['values']);
      final hidden = values['explorer.showHiddenFiles'];
      final ignored = values['explorer.showIgnoredFiles'];
      if (hidden is! bool || ignored is! bool) {
        throw StateError('Shared Explorer preference source unavailable');
      }
      final updated = prefs.copyWith(
        showHiddenFiles: hidden,
        explorerMode: ignored
            ? WorkspaceExplorerMode.showAll
            : WorkspaceExplorerMode.hideIgnored,
      );
      _lastExplorerShared = _explorerSharedValues(updated);
      return updated;
    } catch (error, stackTrace) {
      _lastExplorerShared = null;
      _log.fine('Shared Explorer preferences unavailable', error, stackTrace);
      return prefs;
    }
  }

  /// Migrate Classic's projectId/relativeGitRoot scoped branch favorites
  /// without overriding a prior Native selection or a Host [] tombstone.
  /// Unknown/removed projects stay in old Drift for recovery.
  Future<WorkbenchViewPrefs> _loadGitFavoritesShared(
    WorkbenchViewPrefs prefs,
  ) async {
    try {
      final legacy = <String, Object?>{
        for (final entry in prefs.favoriteBranchesByRepository.entries)
          entry.key: (entry.value.toList()..sort()),
      };
      final response = _asMap(
        await client.runtimeRequest(
          'gitFavorites.seedClassic',
          <String, Object?>{'scopes': legacy},
        ),
      );
      if (response['scopes'] is! Map) {
        throw StateError('Rust Host Git favorites API is unavailable');
      }
      final remote = _asMap(response['scopes']);
      final shared = <String, List<String>>{};
      for (final entry in remote.entries) {
        if (entry.value is List &&
            (entry.value as List).every((item) => item is String)) {
          shared[entry.key] = (entry.value as List).cast<String>().toList();
        }
      }
      final result = prefs.copyWith(
        favoriteBranchesByRepository: {
          ...prefs.favoriteBranchesByRepository,
          for (final entry in shared.entries) entry.key: entry.value.toSet(),
        },
      );
      _lastGitFavoritesShared = shared;
      _lastGitFavoritesView = {
        for (final entry in result.favoriteBranchesByRepository.entries)
          entry.key: entry.value.toSet(),
      };
      return result;
    } catch (error, stackTrace) {
      _lastGitFavoritesShared = null;
      _lastGitFavoritesView = _snapshotFavoriteView(
        prefs.favoriteBranchesByRepository,
      );
      _log.fine('Shared Git favorites Host unavailable', error, stackTrace);
      return prefs;
    }
  }

  Future<WorkbenchViewPrefs> _forRuntime(WorkbenchViewPrefs prefs) async {
    if (prefs.groupBy != WorkbenchGroupBy.section) return prefs;
    try {
      if (client is RuntimeHostCapabilityClient &&
          await (client as RuntimeHostCapabilityClient)
              .supportsRuntimeCapability(
                aleraRuntimeHostWorkspaceSectionsCapability,
              )) {
        return prefs;
      }
    } catch (error, stack) {
      _log.warning('Could not check section support', error, stack);
    }
    return prefs.copyWith(groupBy: WorkbenchGroupBy.project);
  }

  @override
  Future<void> save(WorkbenchViewPrefs prefs) async {
    await beforeAccess?.call();
    final previous = _lastExplorerShared;
    final current = _explorerSharedValues(prefs);
    if (previous != null) {
      final changes = <Map<String, Object?>>[];
      for (final entry in current.entries) {
        if (previous[entry.key] != entry.value) {
          changes.add({
            'path': entry.key,
            'expectedValue': previous[entry.key],
            'value': entry.value,
          });
        }
      }
      if (changes.isNotEmpty) {
        // The two fields are applied atomically in RuntimeStore. On a
        // conflict, keep Drift unchanged and let the caller reload.
        await client.runtimeRequest('desktopSettings.compareAndSetBatch', {
          'changes': changes,
        });
      }
    }
    await _writeGitFavoritesShared(prefs);
    await _writeShared(prefs);
    await legacyRepository.save(prefs);
    if (previous != null) _lastExplorerShared = current;
  }

  /// Update only the Git scopes the user actually changed. Comparing the
  /// *visible* previous selection avoids resending orphaned old Drift keys
  /// from a missing/removed project on unrelated pane layout edits.
  Future<void> _writeGitFavoritesShared(WorkbenchViewPrefs prefs) async {
    final shared = _lastGitFavoritesShared;
    final previousView = _lastGitFavoritesView;
    if (previousView == null) {
      throw const SharedWorkbenchPreferenceUnavailable(
        'Git favorites not loaded',
      );
    }
    final current = prefs.favoriteBranchesByRepository;
    final keys = <String>{...previousView.keys, ...current.keys};
    for (final key in keys) {
      final before = previousView[key] ?? const <String>{};
      final next = current[key] ?? const <String>{};
      if (before.length == next.length && before.containsAll(next)) {
        continue;
      }
      if (shared == null) {
        throw const SharedWorkbenchPreferenceUnavailable('Git favorites');
      }
      final expected = shared[key] ?? <String>[];
      final value = next.toList()..sort();
      Map<String, Object?> response;
      try {
        response = _asMap(
          await client.runtimeRequest(
            'gitFavorites.compareAndSet',
            <String, Object?>{
              'scopeKey': key,
              'expected': expected,
              'value': value,
            },
          ),
        );
      } on TerminalHostConflictException {
        rethrow;
      } catch (_) {
        throw const SharedWorkbenchPreferenceUnavailable('Git favorites');
      }
      final readback = response['branches'];
      if (readback is! List || !readback.every((item) => item is String)) {
        throw const SharedWorkbenchPreferenceUnavailable(
          'Git favorites readback',
        );
      }
      shared[key] = readback.cast<String>().toList();
      previousView[key] = next.toSet();
    }
  }

  Future<void> _writeShared(WorkbenchViewPrefs prefs) async {
    final shared = _sharedJson(prefs);
    if (client is! RuntimeHostCapabilityClient ||
        !await (client as RuntimeHostCapabilityClient)
            .supportsRuntimeCapability(
              aleraRuntimeHostWorkspaceSectionsCapability,
            )) {
      shared.remove('sectionSort');
      shared.remove('collapsedSectionIds');
      shared.remove('othersSectionCollapsed');
      if (prefs.groupBy == WorkbenchGroupBy.section) {
        shared['groupBy'] = 'project';
      }
    }
    final record = _asMap(
      await client.runtimeRequest(
        'workbenchViewPrefs.update',
        <String, Object?>{'expectedRevision': _revision, 'prefs': shared},
      ),
    );
    _revision = (record['revision'] as num?)?.toInt() ?? _revision;
  }
}

Map<String, Set<String>> _snapshotFavoriteView(
  Map<String, Set<String>> favorites,
) => {for (final entry in favorites.entries) entry.key: entry.value.toSet()};

Map<String, bool> _explorerSharedValues(WorkbenchViewPrefs prefs) => {
  'explorer.showHiddenFiles': prefs.showHiddenFiles,
  'explorer.showIgnoredFiles':
      prefs.explorerMode == WorkspaceExplorerMode.showAll,
};

Map<String, Object?> _sharedJson(WorkbenchViewPrefs prefs) {
  return <String, Object?>{
    'groupBy': prefs.groupBy.name,
    'sectionSort': prefs.sectionSort.name,
    'collapsedSectionIds': prefs.collapsedSectionIds.toList(),
    'othersSectionCollapsed': prefs.othersSectionCollapsed,
    'projectSort': prefs.projectSort.name,
    'workspaceSort': prefs.workspaceSort.name,
    'selectedProjectIds': prefs.selectedProjectIds.toList(),
    'selectedTagIds': prefs.selectedTagIds.toList(),
    'collapsedProjectIds': prefs.collapsedProjectIds.toList(),
    'collapsedParentWorkspaceIds': prefs.collapsedParentWorkspaceIds.toList(),
    'pinnedSectionCollapsed': prefs.pinnedSectionCollapsed,
    'allSectionCollapsed': prefs.allSectionCollapsed,
    'showPinnedWorkspacesBelow': prefs.showPinnedWorkspacesBelow,
    'workspaceKindFilter': prefs.workspaceKindFilter.name,
    'showActiveWorkspacesOnly': prefs.showActiveWorkspacesOnly,
  };
}

WorkbenchViewPrefs _mergeShared(
  WorkbenchViewPrefs local,
  Map<String, Object?> shared,
) {
  return local.copyWith(
    groupBy: _enumByName(
      WorkbenchGroupBy.values,
      shared['groupBy'],
      local.groupBy,
    ),
    sectionSort: _enumByName(
      WorkbenchSortBy.values,
      shared['sectionSort'],
      local.sectionSort,
    ),
    collapsedSectionIds: shared.containsKey('collapsedSectionIds')
        ? _stringSet(shared['collapsedSectionIds'])
        : local.collapsedSectionIds,
    othersSectionCollapsed:
        shared['othersSectionCollapsed'] as bool? ??
        local.othersSectionCollapsed,
    projectSort: _enumByName(
      WorkbenchSortBy.values,
      shared['projectSort'],
      local.projectSort,
    ),
    workspaceSort: _enumByName(
      WorkbenchSortBy.values,
      shared['workspaceSort'],
      local.workspaceSort,
    ),
    selectedProjectIds: _stringSet(shared['selectedProjectIds']),
    selectedTagIds: _stringSet(shared['selectedTagIds']),
    collapsedProjectIds: _stringSet(shared['collapsedProjectIds']),
    collapsedParentWorkspaceIds: _stringSet(
      shared['collapsedParentWorkspaceIds'],
    ),
    pinnedSectionCollapsed: shared['pinnedSectionCollapsed'] == true,
    allSectionCollapsed: shared['allSectionCollapsed'] == true,
    showPinnedWorkspacesBelow:
        shared['showPinnedWorkspacesBelow'] as bool? ??
        local.showPinnedWorkspacesBelow,
    workspaceKindFilter: _enumByName(
      WorkspaceKindFilter.values,
      shared['workspaceKindFilter'],
      local.workspaceKindFilter,
    ),
    showActiveWorkspacesOnly:
        shared['showActiveWorkspacesOnly'] as bool? ??
        local.showActiveWorkspacesOnly,
  );
}

T _enumByName<T extends Enum>(List<T> values, Object? value, T fallback) {
  if (value is String) {
    for (final item in values) {
      if (item.name == value) return item;
    }
  }
  return fallback;
}

Set<String> _stringSet(Object? value) {
  return <String>{
    if (value is List)
      for (final item in value)
        if (item is String) item,
  };
}

Map<String, Object?> _asMap(Object? value) {
  if (value is Map<String, Object?>) return value;
  if (value is Map) return Map<String, Object?>.from(value);
  return const <String, Object?>{};
}
