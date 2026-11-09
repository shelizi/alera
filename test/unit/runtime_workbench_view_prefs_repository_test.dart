import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/infra/runtime_workbench_view_prefs_repository.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'sections roundtrip on supported hosts and are omitted on older hosts',
    () async {
      final client = _FakeRuntimeHostClient()..supportsSections = true;
      client.responses['workbenchViewPrefs.get'] = {
        'revision': 4,
        'desktopInitialized': true,
        'prefs': {
          'groupBy': 'section',
          'sectionSort': 'recent',
          'collapsedSectionIds': ['s'],
          'othersSectionCollapsed': true,
        },
      };
      final repo = RuntimeWorkbenchViewPrefsRepository(
        client: client,
        legacyRepository: _MemoryViewPrefsRepository(),
      );
      final prefs = await repo.load();
      expect(prefs.groupBy, WorkbenchGroupBy.section);
      expect(prefs.sectionSort, WorkbenchSortBy.recent);
      expect(prefs.collapsedSectionIds, {'s'});
      await repo.save(prefs);
      expect(
        client.payloads['workbenchViewPrefs.update']!.last['prefs'],
        containsPair('groupBy', 'section'),
      );
      expect(
        client.payloads['workbenchViewPrefs.update']!.last['prefs'],
        containsPair('othersSectionCollapsed', true),
      );
      client.supportsSections = false;
      final fallback = await repo.load();
      expect(fallback.groupBy, WorkbenchGroupBy.project);
      expect(fallback.sectionSort, WorkbenchSortBy.recent);
      expect(fallback.collapsedSectionIds, {'s'});
      await repo.save(prefs);
      final old =
          client.payloads['workbenchViewPrefs.update']!.last['prefs'] as Map;
      expect(old['groupBy'], 'project');
      expect(old.containsKey('sectionSort'), isFalse);
      expect(old.containsKey('collapsedSectionIds'), isFalse);
    },
  );

  test('loads and saves the shared active workspace filter', () async {
    final client = _FakeRuntimeHostClient()
      ..responses['workbenchViewPrefs.get'] = <String, Object?>{
        'revision': 4,
        'desktopInitialized': true,
        'prefs': <String, Object?>{'showActiveWorkspacesOnly': true},
      }
      ..responses['workbenchViewPrefs.update'] = <String, Object?>{
        'revision': 5,
      };
    final legacy = _MemoryViewPrefsRepository();
    final repository = RuntimeWorkbenchViewPrefsRepository(
      client: client,
      legacyRepository: legacy,
    );

    final loaded = await repository.load();
    expect(loaded.showActiveWorkspacesOnly, isTrue);

    await repository.save(loaded.copyWith(showActiveWorkspacesOnly: false));
    expect(
      client.payloads['workbenchViewPrefs.update']?.single['prefs'],
      containsPair('showActiveWorkspacesOnly', false),
    );
    expect(legacy.prefs.showActiveWorkspacesOnly, isFalse);
  });

  test(
    'Explorer visibility is Host-owned while Classic pane layout stays local',
    () async {
      final client = _FakeRuntimeHostClient()
        ..responses['workbenchViewPrefs.get'] = <String, Object?>{
          'revision': 2,
          'desktopInitialized': true,
          'prefs': <String, Object?>{},
        }
        ..responses['workbenchViewPrefs.update'] = <String, Object?>{
          'revision': 3,
        }
        ..responses['desktopSettings.seedLocal'] = <String, Object?>{
          'values': <String, Object?>{
            'explorer.showHiddenFiles': false,
            'explorer.showIgnoredFiles': false,
          },
        };
      final legacy = _MemoryViewPrefsRepository()
        ..prefs = WorkbenchViewPrefs.defaults.copyWith(
          showHiddenFiles: true,
          explorerMode: WorkspaceExplorerMode.showAll,
          rightSidebarWidth: 355,
        );
      final repo = RuntimeWorkbenchViewPrefsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final loaded = await repo.load();
      expect(loaded.showHiddenFiles, false);
      expect(loaded.explorerMode, WorkspaceExplorerMode.hideIgnored);
      expect(loaded.rightSidebarWidth, 355);
      expect(client.payloads['desktopSettings.seedLocal']?.single['values'], {
        'explorer.showHiddenFiles': true,
        'explorer.showIgnoredFiles': true,
      });
      await repo.save(loaded.copyWith(rightSidebarWidth: 400));
      expect(
        client.payloads['desktopSettings.compareAndSetBatch'],
        isNull,
        reason:
            'Classic-only pane sizes never modify shared Explorer preferences',
      );
      final changed = loaded.copyWith(
        showHiddenFiles: true,
        explorerMode: WorkspaceExplorerMode.showAll,
      );
      await repo.save(changed);
      expect(client.payloads['desktopSettings.compareAndSetBatch'], [
        {
          'changes': [
            {
              'path': 'explorer.showHiddenFiles',
              'expectedValue': false,
              'value': true,
            },
            {
              'path': 'explorer.showIgnoredFiles',
              'expectedValue': false,
              'value': true,
            },
          ],
        },
      ]);
      expect(legacy.prefs.showHiddenFiles, true);
      expect(legacy.prefs.explorerMode, WorkspaceExplorerMode.showAll);
    },
  );

  test(
    'Stale Classic Explorer changes cannot overwrite GPUI or local Drift cache',
    () async {
      final client = _FakeRuntimeHostClient()
        ..responses['workbenchViewPrefs.get'] = <String, Object?>{
          'revision': 1,
          'desktopInitialized': true,
          'prefs': <String, Object?>{},
        }
        ..responses['desktopSettings.seedLocal'] = <String, Object?>{
          'values': <String, Object?>{
            'explorer.showHiddenFiles': false,
            'explorer.showIgnoredFiles': true,
          },
        };
      final legacy = _MemoryViewPrefsRepository();
      final repo = RuntimeWorkbenchViewPrefsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final loaded = await repo.load();
      client.failExplorerBatch = true;
      await expectLater(
        repo.save(loaded.copyWith(showHiddenFiles: true)),
        throwsStateError,
      );
      expect(legacy.prefs.showHiddenFiles, false);
      expect(legacy.prefs.explorerMode, WorkspaceExplorerMode.showAll);
      expect(client.payloads['workbenchViewPrefs.update'], isNull);
    },
  );

  test('Git favorites migrate with project and nested repo identities while UI layout stays local', () async {
    const root = '["proj-a",null]';
    const nested = '["proj-a","packages/subrepo"]';
    const orphan = '["removed",null]';
    final client = _FakeRuntimeHostClient()
      ..responses['workbenchViewPrefs.get'] = {
        'revision': 4,
        'desktopInitialized': true,
        'prefs': <String, Object?>{},
      }
      ..responses['desktopSettings.seedLocal'] = {
        'values': {
          'explorer.showHiddenFiles': false,
          'explorer.showIgnoredFiles': false,
        },
      }
      ..responses['gitFavorites.seedClassic'] = {
        'scopes': <String, Object?>{
          root: ['main', 'feature/native'],
          nested: ['nested-release'],
        },
      }
      ..responses['gitFavorites.compareAndSet'] = {
        'scopeKey': root,
        'branches': ['feature/new', 'main'],
      };
    final legacy = _MemoryViewPrefsRepository()
      ..prefs = WorkbenchViewPrefs.defaults.copyWith(
        rightSidebarWidth: 333,
        favoriteBranchesByRepository: {
          root: {'main', 'feature/classic-old'},
          nested: {'nested-release'},
          orphan: {'keep-in-legacy'},
        },
      );
    final repo = RuntimeWorkbenchViewPrefsRepository(
      client: client,
      legacyRepository: legacy,
    );
    final loaded = await repo.load();
    expect(client.payloads['gitFavorites.seedClassic']?.single['scopes'], {
      root: ['feature/classic-old', 'main'],
      nested: ['nested-release'],
      orphan: ['keep-in-legacy'],
    });
    expect(loaded.favoriteBranchesByRepository[root], {
      'main',
      'feature/native',
    });
    expect(loaded.favoriteBranchesByRepository[nested], {'nested-release'});
    expect(loaded.favoriteBranchesByRepository[orphan], {'keep-in-legacy'});
    expect(loaded.rightSidebarWidth, 333);

    await repo.save(loaded.copyWith(rightSidebarWidth: 450));
    expect(
      client.payloads['gitFavorites.compareAndSet'],
      isNull,
      reason: 'Changing Classic-only pane geometry cannot write favorites',
    );

    await repo.save(
      loaded.copyWith(
        favoriteBranchesByRepository: {
          ...loaded.favoriteBranchesByRepository,
          root: {'feature/new', 'main'},
        },
      ),
    );
    expect(client.payloads['gitFavorites.compareAndSet'], [
      {
        'scopeKey': root,
        'expected': ['main', 'feature/native'],
        'value': ['feature/new', 'main'],
      },
    ]);
    expect(legacy.prefs.favoriteBranchesByRepository[root], {
      'feature/new',
      'main',
    });
    expect(legacy.prefs.favoriteBranchesByRepository[nested], {
      'nested-release',
    });
    expect(legacy.prefs.favoriteBranchesByRepository[orphan], {
      'keep-in-legacy',
    });
  });

  test(
    'Offline Host cannot create another Classic Git favorites store',
    () async {
      const scope = '["project-a",null]';
      final client = _FakeRuntimeHostClient()
        ..responses['workbenchViewPrefs.get'] = {
          'revision': 3,
          'desktopInitialized': true,
          'prefs': <String, Object?>{},
        }
        ..responses['desktopSettings.seedLocal'] = {
          'values': {
            'explorer.showHiddenFiles': false,
            'explorer.showIgnoredFiles': false,
          },
        };
      // Old Host returns no Git favorites endpoint response.
      final legacy = _MemoryViewPrefsRepository()
        ..prefs = WorkbenchViewPrefs.defaults.copyWith(
          favoriteBranchesByRepository: {
            scope: {'main'},
          },
        );
      final repo = RuntimeWorkbenchViewPrefsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final loaded = await repo.load();
      await expectLater(
        repo.save(
          loaded.copyWith(
            favoriteBranchesByRepository: {
              scope: {'main', 'offline-feature'},
            },
          ),
        ),
        throwsA(isA<SharedWorkbenchPreferenceUnavailable>()),
      );
      expect(legacy.prefs.favoriteBranchesByRepository[scope], {'main'});
      expect(client.payloads['gitFavorites.compareAndSet'], isNull);
      expect(client.payloads['workbenchViewPrefs.update'], isNull);
    },
  );

  test(
    'Stale Classic Git favorite write is rejected without touching Drift',
    () async {
      const scope = '["proj-a",null]';
      final client = _FakeRuntimeHostClient()
        ..responses['workbenchViewPrefs.get'] = {
          'revision': 1,
          'desktopInitialized': true,
          'prefs': {},
        }
        ..responses['desktopSettings.seedLocal'] = {
          'values': {
            'explorer.showHiddenFiles': false,
            'explorer.showIgnoredFiles': false,
          },
        }
        ..responses['gitFavorites.seedClassic'] = {
          'scopes': {
            scope: ['main'],
          },
        };
      final legacy = _MemoryViewPrefsRepository();
      final repo = RuntimeWorkbenchViewPrefsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final loaded = await repo.load();
      client.failFavoritesCas = true;
      await expectLater(
        repo.save(
          loaded.copyWith(
            favoriteBranchesByRepository: {
              scope: {'main', 'stale'},
            },
          ),
        ),
        throwsA(isA<TerminalHostConflictException>()),
      );
      expect(legacy.prefs.favoriteBranchesByRepository[scope], {'main'});
      expect(client.payloads['workbenchViewPrefs.update'], isNull);
    },
  );
}

final class _MemoryViewPrefsRepository implements WorkbenchViewPrefsRepository {
  WorkbenchViewPrefs prefs = .defaults;

  @override
  Stream<WorkbenchViewPrefs> get changes =>
      const Stream<WorkbenchViewPrefs>.empty();

  @override
  Future<WorkbenchViewPrefs> load() async => prefs;

  @override
  Future<void> save(WorkbenchViewPrefs prefs) async {
    this.prefs = prefs;
  }
}

final class _FakeRuntimeHostClient
    implements RuntimeHostClient, RuntimeHostCapabilityClient {
  bool supportsSections = false;
  bool failExplorerBatch = false;
  bool failFavoritesCas = false;
  @override
  Future<bool> supportsRuntimeCapability(String capability) async =>
      supportsSections && capability == 'workspaceSectionsV1';
  final responses = <String, Object?>{};
  final payloads = <String, List<Map<String, Object?>>>{};

  @override
  Stream<RuntimeHostEvent> get runtimeEvents =>
      const Stream<RuntimeHostEvent>.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    payloads.putIfAbsent(type, () => <Map<String, Object?>>[]).add(payload);
    if (type == 'desktopSettings.compareAndSetBatch' && failExplorerBatch) {
      throw StateError('desktop_settings_revision_conflict');
    }
    if (type == 'gitFavorites.compareAndSet' && failFavoritesCas) {
      throw const TerminalHostConflictException(
        code: 'git_favorites_revision_conflict',
        message: 'Git favorites changed on another desktop',
      );
    }
    return responses[type];
  }
}
