import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workbench_replaceable_tab_editor_sessions.dart';
import 'package:alera/src/rust/api/clipboard.dart' as clipboard_native;
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/rust/api/merman_viewer.dart' as merman_native;
import 'package:alera/src/shared/infra/git/git_explorer_status.dart';
import 'package:flutter/foundation.dart';
import 'package:path/path.dart' as p;

part 'editor_session_registry.dart';

enum WorkspaceFileClipboardOperation { copy, cut }

class const WorkspaceFileClipboardPayload({
  required final List<String> paths,
  required final WorkspaceFileClipboardOperation operation,
  required final int sequenceNumber,
});

class WorkspaceFileService {
  static const _quickOpenQueryLimit = 24;
  static const _quickOpenResultLimit = 500;
  static const _quickOpenResultAge = Duration(seconds: 2);
  bool _disposed = false;
  final Map<String, _WorkspaceQuickOpenCacheEntry> _quickOpenCache =
      <String, _WorkspaceQuickOpenCacheEntry>{};
  final Map<String, Timer> _quickOpenRefreshTimers = <String, Timer>{};
  final StreamController<String> _quickOpenRefreshEvents =
      StreamController<String>.broadcast();
  final Map<String, DateTime> _lastOnDemandQuickOpenRefresh =
      <String, DateTime>{};

  /// Emits the workspace path after a background index rebuild completes.
  /// Consumers can update already-visible search results without requiring
  /// the user to type again.
  Stream<String> get quickOpenRefreshes => _quickOpenRefreshEvents.stream;
  Future<List<native.WorkspaceFileEntry>> listChildren({
    required String workspacePath,
    required String relativePath,
    required bool hideIgnored,
    bool hideHidden = false,
  }) {
    return native.listWorkspaceChildren(
      workspacePath: workspacePath,
      relativePath: relativePath,
      hideIgnored: hideIgnored,
      hideHidden: hideHidden,
    );
  }

  Future<native.WorkspaceQuickOpenSession> startQuickOpenSession({
    required String workspacePath,
    required List<String> excludedDirectories,
  }) {
    return native.startWorkspaceQuickOpenSession(
      workspacePath: workspacePath,
      excludedDirectories: excludedDirectories,
    );
  }

  Future<List<native.WorkspaceQuickOpenMatch>> searchQuickOpenSession({
    required native.WorkspaceQuickOpenSession session,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) {
    if (includeDirectories) {
      return native.searchWorkspaceExplorerFilterSession(
        session: session,
        query: query,
        limit: limit,
        includeGitignored: includeGitignored,
      );
    }
    return native.searchWorkspaceQuickOpenSession(
      session: session,
      query: query,
      limit: limit,
      includeGitignored: includeGitignored,
    );
  }

  Future<void> stopQuickOpenSession({
    required native.WorkspaceQuickOpenSession session,
  }) {
    return native.stopWorkspaceQuickOpenSession(session: session);
  }

  List<native.WorkspaceQuickOpenMatch>? peekQuickOpenMatches({
    required String workspacePath,
    required List<String> excludedDirectories,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) {
    final entry =
        _quickOpenCache[_quickOpenCacheKey(workspacePath, excludedDirectories)];
    return entry
        ?.matches[_quickOpenSearchKey(
          query,
          includeGitignored,
          limit,
          includeDirectories,
        )]
        ?.matches;
  }

  Future<List<native.WorkspaceQuickOpenMatch>> searchQuickOpenCached({
    required String workspacePath,
    required List<String> excludedDirectories,
    required String query,
    required bool includeGitignored,
    int limit = 50,
    bool includeDirectories = false,
  }) {
    if (_disposed) {
      return Future.error(StateError('WorkspaceFileService is disposed'));
    }
    final entry = _quickOpenEntry(workspacePath, excludedDirectories);
    final searchKey = _quickOpenSearchKey(
      query,
      includeGitignored,
      limit,
      includeDirectories,
    );
    final request = _WorkspaceQuickOpenSearchRequest(
      key: searchKey,
      query: query,
      includeGitignored: includeGitignored,
      limit: limit,
      includeDirectories: includeDirectories,
    );
    // Oversized result sets are not retained or replayed on every rebuild.
    final retain = limit > 0 && limit <= _quickOpenResultLimit;
    if (retain) {
      entry.searchRequests.remove(searchKey);
      entry.searchRequests[searchKey] = request;
      while (entry.searchRequests.length > _quickOpenQueryLimit) {
        final oldest = entry.searchRequests.keys.first;
        entry.searchRequests.remove(oldest);
        entry.matches.remove(oldest);
      }
      final cached = entry.matches[searchKey];
      if (entry.session != null &&
          cached != null &&
          cached.age.elapsed < _quickOpenResultAge) {
        return Future.value(cached.matches);
      }
    }
    final pending = entry.inFlight[searchKey];
    if (pending != null) {
      return pending;
    }
    late final Future<List<native.WorkspaceQuickOpenMatch>> next;
    next = _searchQuickOpenRequest(entry, request).whenComplete(() {
      if (identical(entry.inFlight[searchKey], next)) {
        entry.inFlight.remove(searchKey);
      }
    });
    entry.inFlight[searchKey] = next;
    return next;
  }

  Future<List<native.WorkspaceQuickOpenMatch>> _searchQuickOpenRequest(
    _WorkspaceQuickOpenCacheEntry entry,
    _WorkspaceQuickOpenSearchRequest request,
  ) async {
    final searchKey = request.key;
    final query = request.query;
    final includeGitignored = request.includeGitignored;
    final limit = request.limit;
    final includeDirectories = request.includeDirectories;
    var recoveredEviction = false;
    // Revalidate after each await: a background refresh may replace the
    // native snapshot while this search is still in flight.
    for (var attempt = 0; attempt < 3; attempt++) {
      final session = await _ensureQuickOpenSession(entry);
      if (_disposed) throw StateError('WorkspaceFileService is disposed');
      final cached = entry.matches[searchKey];
      if (entry.session?.id == session.id &&
          cached != null &&
          cached.age.elapsed < _quickOpenResultAge) {
        return cached.matches;
      }
      List<native.WorkspaceQuickOpenMatch> matches;
      try {
        matches = await searchQuickOpenSession(
          session: session,
          query: query,
          includeGitignored: includeGitignored,
          limit: limit,
          includeDirectories: includeDirectories,
        );
      } on native.WorkspaceFileError catch (error) {
        // Rust evicts idle sessions and caps the session registry. Rebuild
        // once for a missing session, but never suppress unrelated errors.
        if (recoveredEviction ||
            error.kind != native.WorkspaceFileErrorKind.notFound ||
            error.context != 'quick open session not found') {
          rethrow;
        }
        recoveredEviction = true;
        if (entry.session?.id == session.id) {
          entry.session = null;
          entry.matches.clear();
        }
        continue;
      }
      if (_disposed) throw StateError('WorkspaceFileService is disposed');
      if (entry.session?.id != session.id) continue;
      final frozen = List<native.WorkspaceQuickOpenMatch>.unmodifiable(matches);
      if (entry.searchRequests.containsKey(searchKey)) {
        entry.matches[searchKey] = _WorkspaceQuickOpenCachedResult(frozen);
      }
      return frozen;
    }
    throw StateError('Quick open index changed repeatedly during search');
  }

  Future<void> warmQuickOpenSession({
    required String workspacePath,
    required List<String> excludedDirectories,
  }) async {
    await searchQuickOpenCached(
      workspacePath: workspacePath,
      excludedDirectories: excludedDirectories,
      query: '',
      includeGitignored: false,
    );
  }

  void scheduleQuickOpenRefresh({
    required String workspacePath,
    Duration debounce = const Duration(milliseconds: 120),
  }) {
    if (_disposed) return;
    _quickOpenRefreshTimers.remove(workspacePath)?.cancel();
    _quickOpenRefreshTimers[workspacePath] = Timer(debounce, () {
      _quickOpenRefreshTimers.remove(workspacePath);
      unawaited(_refreshQuickOpenEntries(workspacePath));
    });
  }

  /// An explorer filter can search files in collapsed, unwatched directories.
  /// Refresh its index in the background on demand, but do not reindex the
  /// entire workspace for every keystroke.
  void scheduleQuickOpenFilterRefresh({required String workspacePath}) {
    if (_disposed) return;
    final now = DateTime.now();
    final last = _lastOnDemandQuickOpenRefresh[workspacePath];
    if (last != null && now.difference(last) < const Duration(seconds: 3)) {
      return;
    }
    _lastOnDemandQuickOpenRefresh[workspacePath] = now;
    scheduleQuickOpenRefresh(
      workspacePath: workspacePath,
      debounce: const Duration(milliseconds: 200),
    );
  }

  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    for (final timer in _quickOpenRefreshTimers.values) {
      timer.cancel();
    }
    _quickOpenRefreshTimers.clear();
    _lastOnDemandQuickOpenRefresh.clear();
    final sessions = <native.WorkspaceQuickOpenSession>[
      for (final entry in _quickOpenCache.values)
        if (entry.session != null) entry.session!,
    ];
    _quickOpenCache.clear();
    await _quickOpenRefreshEvents.close();
    await Future.wait(
      sessions.map((session) async {
        try {
          await stopQuickOpenSession(session: session);
        } catch (_) {
          // Service disposal is best effort.
        }
      }),
    );
  }

  _WorkspaceQuickOpenCacheEntry _quickOpenEntry(
    String workspacePath,
    List<String> excludedDirectories,
  ) {
    final key = _quickOpenCacheKey(workspacePath, excludedDirectories);
    return _quickOpenCache.putIfAbsent(
      key,
      () => _WorkspaceQuickOpenCacheEntry(
        workspacePath: workspacePath,
        excludedDirectories: List<String>.unmodifiable(excludedDirectories),
      ),
    );
  }

  Future<native.WorkspaceQuickOpenSession> _ensureQuickOpenSession(
    _WorkspaceQuickOpenCacheEntry entry,
  ) {
    if (_disposed) {
      return Future.error(StateError('WorkspaceFileService is disposed'));
    }
    final current = entry.session;
    if (current != null) {
      return Future<native.WorkspaceQuickOpenSession>.value(current);
    }
    final pending = entry.pendingSession;
    if (pending != null) {
      return pending;
    }
    late final Future<native.WorkspaceQuickOpenSession> next;
    next =
        Future<native.WorkspaceQuickOpenSession>.sync(
          () => startQuickOpenSession(
            workspacePath: entry.workspacePath,
            excludedDirectories: entry.excludedDirectories,
          ),
        ).then(
          (session) {
            if (_disposed || !identical(entry.pendingSession, next)) {
              unawaited(_stopQuickOpenSessionBestEffort(session));
              if (!_disposed && entry.session != null) return entry.session!;
              throw StateError('Quick open session was retired');
            }
            entry
              ..session = session
              ..pendingSession = null;
            _schedulePendingQuickOpenRefresh(entry);
            return session;
          },
          onError: (Object error, StackTrace stackTrace) {
            if (identical(entry.pendingSession, next)) {
              entry.pendingSession = null;
              _schedulePendingQuickOpenRefresh(entry);
            }
            Error.throwWithStackTrace(error, stackTrace);
          },
        );
    entry.pendingSession = next;
    return next;
  }

  void _schedulePendingQuickOpenRefresh(_WorkspaceQuickOpenCacheEntry entry) {
    if (!entry.refreshPending || _quickOpenRefreshEvents.isClosed) {
      return;
    }
    entry.refreshPending = false;
    scheduleQuickOpenRefresh(
      workspacePath: entry.workspacePath,
      debounce: const Duration(milliseconds: 120),
    );
  }

  Future<void> _refreshQuickOpenEntries(String workspacePath) async {
    if (_disposed) return;
    final entries = _quickOpenCache.values
        .where((entry) => entry.workspacePath == workspacePath)
        .toList(growable: false);
    var refreshed = false;
    for (final entry in entries) {
      if (entry.refreshing || entry.pendingSession != null) {
        entry.refreshPending = true;
        continue;
      }
      if (_disposed) break;
      entry.refreshing = true;
      native.WorkspaceQuickOpenSession? replacementSession;
      try {
        final session = await startQuickOpenSession(
          workspacePath: entry.workspacePath,
          excludedDirectories: entry.excludedDirectories,
        );
        replacementSession = session;
        if (_disposed) continue;
        final refreshedMatches = <String, _WorkspaceQuickOpenCachedResult>{};
        // At most the latest request for each file/directory and ignored
        // mode. Other recent queries are invalidated, not replayed. Active
        // consumers requery on the refresh event, including requests that
        // arrived while this asynchronous rebuild was running.
        final seenModes = <(bool, bool)>{};
        final recent = entry.searchRequests.values
            .toList()
            .reversed
            .where(
              (request) => seenModes.add((
                request.includeDirectories,
                request.includeGitignored,
              )),
            )
            .toList(growable: false);
        for (final request in recent) {
          final matches = await searchQuickOpenSession(
            session: session,
            query: request.query,
            includeGitignored: request.includeGitignored,
            limit: request.limit,
            includeDirectories: request.includeDirectories,
          );
          refreshedMatches[request.key] = _WorkspaceQuickOpenCachedResult(
            List<native.WorkspaceQuickOpenMatch>.unmodifiable(matches),
          );
          if (_disposed) break;
        }
        if (_disposed) continue;
        final previousSession = entry.session;
        entry
          ..session = session
          ..pendingSession = null;
        replacementSession = null;
        // Never let an old in-flight query satisfy a new-generation request.
        entry.inFlight.clear();
        refreshedMatches.removeWhere(
          (key, _) => !entry.searchRequests.containsKey(key),
        );
        entry.matches
          ..clear()
          ..addAll(refreshedMatches);
        refreshed = true;
        if (previousSession != null && previousSession.id != session.id) {
          unawaited(_stopQuickOpenSessionBestEffort(previousSession));
        }
      } catch (_) {
        // Keep serving the previous index if a background refresh fails.
      } finally {
        entry.refreshing = false;
        if (replacementSession != null) {
          unawaited(_stopQuickOpenSessionBestEffort(replacementSession));
        }
        _schedulePendingQuickOpenRefresh(entry);
      }
    }
    if (refreshed && !_quickOpenRefreshEvents.isClosed) {
      _quickOpenRefreshEvents.add(workspacePath);
    }
  }

  Future<void> _stopQuickOpenSessionBestEffort(
    native.WorkspaceQuickOpenSession session,
  ) async {
    try {
      await stopQuickOpenSession(session: session);
    } catch (_) {
      // Superseded cache sessions are cleanup-only work.
    }
  }

  String _quickOpenCacheKey(
    String workspacePath,
    List<String> excludedDirectories,
  ) => '$workspacePath\u0000${excludedDirectories.join('\u0000')}';

  String _quickOpenSearchKey(
    String query,
    bool includeGitignored,
    int limit,
    bool includeDirectories,
  ) =>
      '${includeDirectories ? 1 : 0}:${includeGitignored ? 1 : 0}:$limit:${query.trim()}';

  List<native.WorkspaceFileEntry> applyGitStatusSnapshot(
    List<native.WorkspaceFileEntry> entries,
    GitExplorerStatusSnapshot snapshot,
  ) {
    List<native.WorkspaceFileEntry>? updated;
    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final status = snapshot.statusFor(entry.relativePath);
      if (status == null) {
        continue;
      }
      final gitStatus = switch (status) {
        GitExplorerStatus.untracked => native.WorkspaceFileGitStatus.untracked,
        GitExplorerStatus.added => native.WorkspaceFileGitStatus.added,
        GitExplorerStatus.modified => native.WorkspaceFileGitStatus.modified,
      };
      if (entry.gitStatus == gitStatus) {
        continue;
      }
      updated ??= List<native.WorkspaceFileEntry>.of(entries, growable: false);
      updated[index] = native.WorkspaceFileEntry(
        relativePath: entry.relativePath,
        name: entry.name,
        kind: entry.kind,
        size: entry.size,
        modifiedMillis: entry.modifiedMillis,
        contentToken: entry.contentToken,
        isIgnored: entry.isIgnored,
        isHidden: entry.isHidden,
        isSymlink: entry.isSymlink,
        isProtected: entry.isProtected,
        hasChildrenHint: entry.hasChildrenHint,
        gitStatus: gitStatus,
      );
    }
    return updated ?? entries;
  }

  Future<native.WorkspaceExplorerTreeProjection> projectExplorerTree({
    required String workspaceName,
    required String workspacePath,
    required List<native.WorkspaceExplorerDirectoryChildren> directories,
    native.WorkspaceExplorerDirectoryChildren? replacement,
  }) {
    return native.projectWorkspaceExplorerTree(
      workspaceName: workspaceName,
      workspacePath: workspacePath,
      directories: directories,
      replacement: replacement,
    );
  }

  /// One-time seed of the Rust-owned tree. Changes send only the modified
  /// directory and a versioned handle rather than all loaded snapshots.
  Future<native.WorkspaceExplorerProjectionHandle>
  startExplorerProjectionSession({
    required String workspaceName,
    required String workspacePath,
    required List<native.WorkspaceExplorerDirectoryChildren> directories,
  }) {
    return native.startWorkspaceExplorerProjectionSession(
      workspaceName: workspaceName,
      workspacePath: workspacePath,
      directories: directories,
    );
  }

  Future<native.WorkspaceExplorerProjectionDelta>
  updateExplorerProjectionSession({
    required native.WorkspaceExplorerProjectionHandle handle,
    required native.WorkspaceExplorerDirectoryChildren replacement,
  }) {
    return native.updateWorkspaceExplorerProjectionSession(
      handle: handle,
      replacement: replacement,
    );
  }

  Future<void> stopExplorerProjectionSession({
    required native.WorkspaceExplorerProjectionHandle handle,
  }) {
    return native.stopWorkspaceExplorerProjectionSession(handle: handle);
  }

  Future<native.WorkspaceExplorerWatcherHandle> startExplorerWatcher({
    required String workspacePath,
  }) {
    return native.startWorkspaceExplorerWatcher(workspacePath: workspacePath);
  }

  Future<void> updateExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
    required List<String> watchedRelativePaths,
  }) {
    return native.updateWorkspaceExplorerWatcher(
      handle: handle,
      watchedRelativePaths: watchedRelativePaths,
    );
  }

  Stream<native.WorkspaceExplorerWatchBatch> watchExplorerEvents({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) {
    return native.watchWorkspaceExplorerEvents(handle: handle);
  }

  Future<void> stopExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) {
    return native.stopWorkspaceExplorerWatcher(handle: handle);
  }

  Future<native.WorkspaceTextFile> readTextFile({
    required String workspacePath,
    required String relativePath,
  }) {
    return native.readWorkspaceTextFile(
      workspacePath: workspacePath,
      relativePath: relativePath,
    );
  }

  /// Reads the same lightweight metadata token used by the Rust workspace
  /// writer without reading the file contents. This keeps external-change
  /// filtering cheap even for very large open files.
  Future<String?> contentTokenForFile({
    required String workspacePath,
    required String relativePath,
  }) async {
    final root = p.normalize(p.absolute(workspacePath));
    final path = p.normalize(p.join(root, relativePath));
    if (!p.isWithin(root, path)) {
      return null;
    }
    try {
      final stat = await FileStat.stat(path);
      if (stat.type != FileSystemEntityType.file) {
        return null;
      }
      return '${stat.size}:${stat.modified.millisecondsSinceEpoch}';
    } on FileSystemException {
      return null;
    }
  }

  Future<merman_native.MermanWorkspaceRender> renderMermanWorkspaceFile({
    required String workspacePath,
    required String relativePath,
  }) {
    return merman_native.renderMermanWorkspaceFile(
      workspacePath: workspacePath,
      relativePath: relativePath,
    );
  }

  Future<String> renderMermanSource({
    required String source,
    required String diagramId,
  }) {
    return merman_native.renderMermanSource(
      source: source,
      diagramId: diagramId,
    );
  }

  Future<native.WorkspaceDecodedText> decodeTextBytes({
    required List<int> bytes,
    native.WorkspaceTextEncoding? encoding,
  }) {
    return native.decodeWorkspaceTextBytes(bytes: bytes, encoding: encoding);
  }

  Future<native.WorkspaceEditorTextFile> readEditorTextFile({
    required String workspacePath,
    required String relativePath,
    required int tabSize,
    native.WorkspaceTextEncoding? encoding,
  }) {
    return native.readWorkspaceEditorTextFile(
      workspacePath: workspacePath,
      relativePath: relativePath,
      tabSize: tabSize,
      encoding: encoding,
    );
  }

  Future<native.WorkspaceTextFile> writeTextFile({
    required String workspacePath,
    required String relativePath,
    required String content,
    required String? expectedContentToken,
    required bool overwriteIfChanged,
  }) {
    return native.writeWorkspaceTextFile(
      workspacePath: workspacePath,
      relativePath: relativePath,
      content: content,
      expectedContentToken: expectedContentToken,
      overwriteIfChanged: overwriteIfChanged,
    );
  }

  Future<native.WorkspaceEditorTextFile> writeEditorTextFile({
    required String workspacePath,
    required String relativePath,
    required String currentDisplayContent,
    required String? originalRawContent,
    required String? originalDisplayContent,
    required String? expectedContentToken,
    required bool overwriteIfChanged,
    required int tabSize,
    required native.WorkspaceTextEncoding encoding,
  }) {
    return native.writeWorkspaceEditorTextFile(
      workspacePath: workspacePath,
      relativePath: relativePath,
      currentDisplayContent: currentDisplayContent,
      originalRawContent: originalRawContent,
      originalDisplayContent: originalDisplayContent,
      expectedContentToken: expectedContentToken,
      overwriteIfChanged: overwriteIfChanged,
      tabSize: tabSize,
      encoding: encoding,
    );
  }

  Future<native.WorkspaceFileEntry> createFile({
    required String workspacePath,
    required String parentRelativePath,
    required String name,
  }) {
    return native.createWorkspaceFile(
      workspacePath: workspacePath,
      parentRelativePath: parentRelativePath,
      name: name,
    );
  }

  Future<native.WorkspaceFileEntry> createDirectory({
    required String workspacePath,
    required String parentRelativePath,
    required String name,
  }) {
    return native.createWorkspaceDirectory(
      workspacePath: workspacePath,
      parentRelativePath: parentRelativePath,
      name: name,
    );
  }

  Future<native.WorkspaceFileEntry> renameEntry({
    required String workspacePath,
    required String relativePath,
    required String newName,
  }) {
    return native.renameWorkspaceEntry(
      workspacePath: workspacePath,
      relativePath: relativePath,
      newName: newName,
    );
  }

  Future<native.WorkspaceFileEntry> copyEntry({
    required String workspacePath,
    required String relativePath,
    required String targetParentRelativePath,
  }) {
    return native.copyWorkspaceEntry(
      workspacePath: workspacePath,
      relativePath: relativePath,
      targetParentRelativePath: targetParentRelativePath,
    );
  }

  Future<native.WorkspaceFileEntry> moveEntry({
    required String workspacePath,
    required String relativePath,
    required String targetParentRelativePath,
  }) {
    return native.moveWorkspaceEntry(
      workspacePath: workspacePath,
      relativePath: relativePath,
      targetParentRelativePath: targetParentRelativePath,
    );
  }

  Future<List<native.WorkspaceFileEntry>> importEntries({
    required String workspacePath,
    required List<String> sourcePaths,
    required String targetParentRelativePath,
    required bool moveSources,
  }) {
    return native.importWorkspaceEntries(
      workspacePath: workspacePath,
      sourcePaths: sourcePaths,
      targetParentRelativePath: targetParentRelativePath,
      moveSources: moveSources,
    );
  }

  Future<int?> writeSystemFileClipboard({
    required List<String> paths,
    required WorkspaceFileClipboardOperation operation,
  }) async {
    if (!Platform.isWindows) {
      return null;
    }
    return clipboard_native.setFileClipboard(
      paths: paths,
      operation: operation == WorkspaceFileClipboardOperation.cut
          ? clipboard_native.FileClipboardOperation.cut
          : clipboard_native.FileClipboardOperation.copy,
    );
  }

  Future<WorkspaceFileClipboardPayload?> readSystemFileClipboard() async {
    if (!Platform.isWindows) {
      return null;
    }
    final payload = await clipboard_native.readFileClipboard();
    if (payload == null) {
      return null;
    }
    return WorkspaceFileClipboardPayload(
      paths: List.unmodifiable(payload.paths),
      operation:
          payload.operation == clipboard_native.FileClipboardOperation.cut
          ? WorkspaceFileClipboardOperation.cut
          : WorkspaceFileClipboardOperation.copy,
      sequenceNumber: payload.sequenceNumber,
    );
  }

  Future<int?> systemFileClipboardSequenceNumber() async {
    if (!Platform.isWindows) {
      return null;
    }
    return clipboard_native.fileClipboardSequenceNumber();
  }

  Future<bool> clearSystemFileClipboardIfSequence(int sequenceNumber) async {
    if (!Platform.isWindows) {
      return false;
    }
    return clipboard_native.clearFileClipboardIfSequence(
      sequenceNumber: sequenceNumber,
    );
  }

  Future<void> deleteEntry({
    required String workspacePath,
    required String relativePath,
    bool useTrash = true,
  }) {
    return native.deleteWorkspaceEntry(
      workspacePath: workspacePath,
      relativePath: relativePath,
      useTrash: useTrash,
    );
  }

  Future<ResolvedWorkspaceFile> resolveWorkspaceFilePath({
    required String workspacePath,
    required String relativePath,
  }) async {
    final normalizedRelativePath = _normalizeRelativeFilePath(relativePath);
    late final String workspaceCanonicalPath;
    try {
      workspaceCanonicalPath = await Directory(workspacePath)
          .resolveSymbolicLinks();
    } on FileSystemException catch (error) {
      throw native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.io,
        context: error.message,
      );
    }

    final requestedPath = p.join(
      workspaceCanonicalPath,
      normalizedRelativePath,
    );
    late final String fileCanonicalPath;
    try {
      fileCanonicalPath = await File(requestedPath).resolveSymbolicLinks();
    } on FileSystemException catch (error) {
      throw native.WorkspaceFileError(
        kind: error.osError == null
            ? native.WorkspaceFileErrorKind.io
            : native.WorkspaceFileErrorKind.notFound,
        context: relativePath,
      );
    }

    if (!p.isWithin(workspaceCanonicalPath, fileCanonicalPath)) {
      throw native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.outsideWorkspace,
        context: relativePath,
      );
    }

    final stat = await File(fileCanonicalPath).stat();
    if (stat.type != FileSystemEntityType.file) {
      throw native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.unsupported,
        context: relativePath,
      );
    }
    return ResolvedWorkspaceFile(
      path: fileCanonicalPath,
      modifiedMicros: stat.modified.microsecondsSinceEpoch,
      length: stat.size,
    );
  }

  String _normalizeRelativeFilePath(String relativePath) {
    if (relativePath.isEmpty || p.isAbsolute(relativePath)) {
      throw native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.invalidPath,
        context: relativePath,
      );
    }
    final normalized = p.normalize(relativePath.replaceAll(r'\', p.separator));
    final parts = p.split(normalized);
    if (normalized == '.' || parts.contains('..')) {
      throw native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.invalidPath,
        context: relativePath,
      );
    }
    return normalized;
  }
}

class _WorkspaceQuickOpenCacheEntry {
  _WorkspaceQuickOpenCacheEntry({
    required this.workspacePath,
    required this.excludedDirectories,
  });

  final String workspacePath;
  final List<String> excludedDirectories;
  native.WorkspaceQuickOpenSession? session;
  Future<native.WorkspaceQuickOpenSession>? pendingSession;
  bool refreshing = false;
  bool refreshPending = false;
  final Map<String, _WorkspaceQuickOpenCachedResult> matches =
      <String, _WorkspaceQuickOpenCachedResult>{};
  final Map<String, Future<List<native.WorkspaceQuickOpenMatch>>> inFlight =
      <String, Future<List<native.WorkspaceQuickOpenMatch>>>{};
  final Map<String, _WorkspaceQuickOpenSearchRequest> searchRequests =
      <String, _WorkspaceQuickOpenSearchRequest>{};
}

class _WorkspaceQuickOpenCachedResult {
  _WorkspaceQuickOpenCachedResult(this.matches);
  final List<native.WorkspaceQuickOpenMatch> matches;
  final Stopwatch age = Stopwatch()..start();
}

class const _WorkspaceQuickOpenSearchRequest({
  required final String key,
  required final String query,
  required final bool includeGitignored,
  required final bool includeDirectories,
  required final int limit,
});

class const ResolvedWorkspaceFile({
  required final String path,
  required final int modifiedMicros,
  required final int length,
});
