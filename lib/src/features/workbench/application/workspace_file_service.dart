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
  final Map<String, _WorkspaceQuickOpenCacheEntry> _quickOpenCache =
      <String, _WorkspaceQuickOpenCacheEntry>{};
  final Map<String, Timer> _quickOpenRefreshTimers = <String, Timer>{};
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
  }) {
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
  }) {
    final entry =
        _quickOpenCache[_quickOpenCacheKey(workspacePath, excludedDirectories)];
    return entry?.matches[_quickOpenSearchKey(query, includeGitignored, limit)];
  }

  Future<List<native.WorkspaceQuickOpenMatch>> searchQuickOpenCached({
    required String workspacePath,
    required List<String> excludedDirectories,
    required String query,
    required bool includeGitignored,
    int limit = 50,
  }) async {
    final entry = _quickOpenEntry(workspacePath, excludedDirectories);
    final searchKey = _quickOpenSearchKey(query, includeGitignored, limit);
    entry.searchRequests[searchKey] = _WorkspaceQuickOpenSearchRequest(
      key: searchKey,
      query: query,
      includeGitignored: includeGitignored,
      limit: limit,
    );
    final session = await _ensureQuickOpenSession(entry);
    final matches = await searchQuickOpenSession(
      session: session,
      query: query,
      includeGitignored: includeGitignored,
      limit: limit,
    );
    if (entry.session?.id == session.id) {
      entry.matches[searchKey] =
          List<native.WorkspaceQuickOpenMatch>.unmodifiable(matches);
    }
    return matches;
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
    Duration debounce = const Duration(milliseconds: 500),
  }) {
    _quickOpenRefreshTimers.remove(workspacePath)?.cancel();
    _quickOpenRefreshTimers[workspacePath] = Timer(debounce, () {
      _quickOpenRefreshTimers.remove(workspacePath);
      unawaited(_refreshQuickOpenEntries(workspacePath));
    });
  }

  Future<void> dispose() async {
    for (final timer in _quickOpenRefreshTimers.values) {
      timer.cancel();
    }
    _quickOpenRefreshTimers.clear();
    final sessions = <native.WorkspaceQuickOpenSession>[
      for (final entry in _quickOpenCache.values)
        if (entry.session != null) entry.session!,
    ];
    _quickOpenCache.clear();
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
    final current = entry.session;
    if (current != null) {
      return Future<native.WorkspaceQuickOpenSession>.value(current);
    }
    final pending = entry.pendingSession;
    if (pending != null) {
      return pending;
    }
    final next = startQuickOpenSession(
      workspacePath: entry.workspacePath,
      excludedDirectories: entry.excludedDirectories,
    );
    entry.pendingSession = next;
    return next.then(
      (session) {
        if (identical(entry.pendingSession, next)) {
          entry
            ..session = session
            ..pendingSession = null;
        }
        return session;
      },
      onError: (Object error, StackTrace stackTrace) {
        if (identical(entry.pendingSession, next)) {
          entry.pendingSession = null;
        }
        Error.throwWithStackTrace(error, stackTrace);
      },
    );
  }

  Future<void> _refreshQuickOpenEntries(String workspacePath) async {
    final entries = _quickOpenCache.values
        .where((entry) => entry.workspacePath == workspacePath)
        .toList(growable: false);
    for (final entry in entries) {
      if (entry.refreshing || entry.pendingSession != null) {
        continue;
      }
      entry.refreshing = true;
      try {
        final session = await startQuickOpenSession(
          workspacePath: entry.workspacePath,
          excludedDirectories: entry.excludedDirectories,
        );
        final refreshedMatches =
            <String, List<native.WorkspaceQuickOpenMatch>>{};
        for (final request in entry.searchRequests.values) {
          final matches = await searchQuickOpenSession(
            session: session,
            query: request.query,
            includeGitignored: request.includeGitignored,
            limit: request.limit,
          );
          refreshedMatches[request.key] =
              List<native.WorkspaceQuickOpenMatch>.unmodifiable(matches);
        }
        final previousSession = entry.session;
        entry
          ..session = session
          ..pendingSession = null;
        entry.matches
          ..clear()
          ..addAll(refreshedMatches);
        if (previousSession != null && previousSession.id != session.id) {
          unawaited(_stopQuickOpenSessionBestEffort(previousSession));
        }
      } catch (_) {
        // Keep serving the previous index if a background refresh fails.
      } finally {
        entry.refreshing = false;
      }
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

  String _quickOpenSearchKey(String query, bool includeGitignored, int limit) =>
      '${includeGitignored ? 1 : 0}:$limit:${query.trim()}';

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
  final Map<String, List<native.WorkspaceQuickOpenMatch>> matches =
      <String, List<native.WorkspaceQuickOpenMatch>>{};
  final Map<String, _WorkspaceQuickOpenSearchRequest> searchRequests =
      <String, _WorkspaceQuickOpenSearchRequest>{};
}

class const _WorkspaceQuickOpenSearchRequest({
  required final String key,
  required final String query,
  required final bool includeGitignored,
  required final int limit,
});

class const ResolvedWorkspaceFile({
  required final String path,
  required final int modifiedMicros,
  required final int length,
});
