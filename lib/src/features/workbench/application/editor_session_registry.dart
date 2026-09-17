part of 'workspace_file_service.dart';

class EditorSessionRegistry extends ChangeNotifier
    implements WorkbenchReplaceableTabEditorSessions {
  final Map<String, EditorDocumentSession> _documents =
      <String, EditorDocumentSession>{};
  final Map<String, EditorSessionHandle> _sessions =
      <String, EditorSessionHandle>{};
  final Map<String, _EditorDocumentPath> _pathByTabId =
      <String, _EditorDocumentPath>{};
  final Map<_EditorDocumentPath, Set<String>> _tabIdsByPath =
      <_EditorDocumentPath, Set<String>>{};
  final Map<_EditorDocumentPath, _EditorPathChangeNotifier> _pathNotifiers =
      <_EditorDocumentPath, _EditorPathChangeNotifier>{};
  EditorDocumentSession documentFor(String tabId) {
    return _documents.putIfAbsent(
      tabId,
      () => EditorDocumentSession(onChanged: () => _documentChanged(tabId)),
    );
  }

  Listenable documentChangesForPath({
    required String workspacePath,
    required String relativePath,
  }) {
    final path = _EditorDocumentPath(workspacePath, relativePath);
    return _pathNotifiers.putIfAbsent(path, _EditorPathChangeNotifier.new);
  }

  void register(String tabId, EditorSessionHandle handle) {
    _sessions[tabId] = handle;
  }

  void unregister(String tabId, EditorSessionHandle handle) {
    if (identical(_sessions[tabId], handle)) {
      _sessions.remove(tabId);
    }
  }

  @override
  bool isDirty(String tabId) {
    return _sessions[tabId]?.isDirty() ?? _documents[tabId]?.isDirty ?? false;
  }

  Future<void> save(String tabId) async {
    await _sessions[tabId]?.save();
  }

  Future<int> saveAll(WorkspaceFileService workspaceFiles) async {
    var savedCount = 0;
    final liveSessionIds = _sessions.keys.toList(growable: false);
    for (final tabId in liveSessionIds) {
      if (!isDirty(tabId)) {
        continue;
      }
      await _sessions[tabId]?.save();
      if (!isDirty(tabId)) {
        savedCount += 1;
      }
    }
    for (final entry in _documents.entries) {
      final tabId = entry.key;
      final document = entry.value;
      if (_sessions.containsKey(tabId) ||
          !document.isDirty ||
          !document.canSave ||
          document.workspacePath == null ||
          document.relativePath == null) {
        continue;
      }
      final saved = await workspaceFiles.writeEditorTextFile(
        workspacePath: document.workspacePath!,
        relativePath: document.relativePath!,
        currentDisplayContent: document.currentText ?? '',
        originalRawContent: document.loadedRawText,
        originalDisplayContent: document.loadedText,
        expectedContentToken: document.contentToken,
        overwriteIfChanged: false,
        tabSize: document.tabSize,
        encoding: document.encoding!,
      );
      document.acceptSaved(saved);
      savedCount += 1;
    }
    return savedCount;
  }

  Future<void> discard(String tabId) async {
    await _sessions[tabId]?.discard();
  }

  List<String> dirtyPathsFor({
    required String workspacePath,
    required Iterable<String> relativePaths,
  }) {
    final candidates = relativePaths.toSet();
    final dirtyPaths = <String>{};
    for (final entry in _documents.entries) {
      final tabId = entry.key;
      final document = entry.value;
      final relativePath = document.relativePath;
      if (document.workspacePath != workspacePath ||
          relativePath == null ||
          !candidates.contains(relativePath)) {
        continue;
      }
      if (isDirty(tabId)) {
        dirtyPaths.add(relativePath);
      }
    }
    final sorted = dirtyPaths.toList()..sort();
    return sorted;
  }

  String? dirtyTextForPath({
    required String workspacePath,
    required String relativePath,
  }) {
    final path = _EditorDocumentPath(workspacePath, relativePath);
    final tabIds = _tabIdsByPath[path];
    if (tabIds == null) {
      return null;
    }
    for (final tabId in tabIds) {
      final document = _documents[tabId];
      if (document == null || document.loadError != null || !isDirty(tabId)) {
        continue;
      }
      final currentText = document.currentText;
      if (currentText != null) {
        return currentText;
      }
      final snapshotText = _sessions[tabId]?.snapshotText;
      if (snapshotText != null) {
        return snapshotText();
      }
    }
    return null;
  }

  void reveal(String tabId, WorkspaceEditorRevealTarget target) {
    final handle = _sessions[tabId];
    final reveal = handle?.reveal;
    if (reveal != null) {
      reveal(target);
      return;
    }
    documentFor(tabId).pendingReveal = target;
  }

  WorkspaceEditorRevealTarget? takePendingReveal(String tabId) {
    final document = _documents[tabId];
    final target = document?.pendingReveal;
    if (document != null) {
      document.pendingReveal = null;
    }
    return target;
  }

  Future<void> reloadCleanFiles({
    required String workspacePath,
    required Iterable<String> relativePaths,
  }) async {
    final candidates = relativePaths.toSet();
    for (final entry in _documents.entries) {
      final tabId = entry.key;
      final document = entry.value;
      final relativePath = document.relativePath;
      if (document.workspacePath != workspacePath ||
          relativePath == null ||
          !candidates.contains(relativePath) ||
          isDirty(tabId)) {
        continue;
      }
      final handle = _sessions[tabId];
      final reload = handle?.reload;
      if (reload != null) {
        await reload();
      } else {
        document.clearSnapshot();
      }
    }
  }

  Future<void> reloadExternallyChangedCleanFiles({
    required WorkspaceFileService workspaceFiles,
    required String workspacePath,
    required Iterable<String> relativePaths,
  }) async {
    final candidates = relativePaths.toSet();
    final observedTokens = <String, String?>{};
    for (final entry in _documents.entries) {
      final tabId = entry.key;
      final document = entry.value;
      final relativePath = document.relativePath;
      if (document.workspacePath != workspacePath ||
          relativePath == null ||
          !candidates.contains(relativePath) ||
          isDirty(tabId)) {
        continue;
      }
      final diskToken = observedTokens.containsKey(relativePath)
          ? observedTokens[relativePath]
          : await workspaceFiles.contentTokenForFile(
              workspacePath: workspacePath,
              relativePath: relativePath,
            );
      observedTokens[relativePath] = diskToken;

      // A watcher also observes Alera's own writes. A successful save already
      // adopts the new token, so an equal token is not an external change and
      // must not reset the editor/undo state.
      if (diskToken == null || diskToken == document.contentToken) {
        continue;
      }
      // The editor may have become dirty while the async metadata lookup ran.
      if (isDirty(tabId) ||
          document.workspacePath != workspacePath ||
          document.relativePath != relativePath) {
        continue;
      }
      final handle = _sessions[tabId];
      final reload = handle?.reload;
      if (reload != null) {
        await reload();
      } else {
        document.clearSnapshot();
      }
    }
  }

  void updateDocumentPathsAfterMove({
    required String workspacePath,
    required String oldRelativePath,
    required String newRelativePath,
  }) {
    for (final document in _documents.values) {
      if (document.workspacePath != workspacePath ||
          document.relativePath == null) {
        continue;
      }
      final nextPath = _replacePathPrefix(
        path: document.relativePath!,
        oldPath: oldRelativePath,
        newPath: newRelativePath,
      );
      if (nextPath != null) {
        document.attachFile(
          workspacePath: workspacePath,
          relativePath: nextPath,
        );
      }
    }
  }

  @override
  void forget(String tabId) {
    final hadSession = _sessions.remove(tabId) != null;
    final hadDocument = _documents.remove(tabId) != null;
    final path = _pathByTabId.remove(tabId);
    if (path != null) {
      final tabIds = _tabIdsByPath[path];
      tabIds?.remove(tabId);
      if (tabIds?.isEmpty ?? false) {
        _tabIdsByPath.remove(path);
      }
      _pathNotifiers[path]?.notifyListeners();
      _maybeReleasePathNotifier(path);
    }
    if (hadSession || hadDocument) {
      notifyListeners();
    }
  }

  /// Drops a path notifier once nothing references its path anymore.
  ///
  /// Without this the map grows with every file ever opened for the lifetime
  /// of the session. A notifier still held by a mounted viewer keeps its
  /// listeners, so it stays; a rebuilt viewer obtains a fresh one through
  /// [documentChangesForPath].
  void _maybeReleasePathNotifier(_EditorDocumentPath path) {
    final notifier = _pathNotifiers[path];
    if (notifier == null ||
        notifier.hasActiveListeners ||
        _tabIdsByPath.containsKey(path)) {
      return;
    }
    _pathNotifiers.remove(path);
  }

  void _documentChanged(String tabId) {
    final document = _documents[tabId];
    if (document == null) {
      return;
    }
    final previousPath = _pathByTabId[tabId];
    final nextPath = switch ((document.workspacePath, document.relativePath)) {
      (final String workspacePath, final String relativePath) =>
        _EditorDocumentPath(workspacePath, relativePath),
      _ => null,
    };
    if (previousPath != nextPath) {
      if (previousPath != null) {
        final previousTabIds = _tabIdsByPath[previousPath];
        previousTabIds?.remove(tabId);
        if (previousTabIds?.isEmpty ?? false) {
          _tabIdsByPath.remove(previousPath);
        }
        _pathNotifiers[previousPath]?.notifyListeners();
        _maybeReleasePathNotifier(previousPath);
      }
      if (nextPath != null) {
        _pathByTabId[tabId] = nextPath;
        _tabIdsByPath.putIfAbsent(nextPath, () => <String>{}).add(tabId);
      } else {
        _pathByTabId.remove(tabId);
      }
    }
    if (nextPath != null) {
      _pathNotifiers[nextPath]?.notifyListeners();
    }
    notifyListeners();
  }

  String? _replacePathPrefix({
    required String path,
    required String oldPath,
    required String newPath,
  }) {
    if (path == oldPath) {
      return newPath;
    }
    final prefix = '$oldPath/';
    if (!path.startsWith(prefix)) {
      return null;
    }
    return '$newPath/${path.substring(prefix.length)}';
  }
}

/// Exposes the protected listener count so the registry can drop notifiers
/// nobody references anymore.
class _EditorPathChangeNotifier extends ChangeNotifier {
  bool get hasActiveListeners => hasListeners;
}

class const _EditorDocumentPath(
  final String workspacePath,
  final String relativePath,
) {
  @override
  bool operator ==(Object other) {
    return other is _EditorDocumentPath &&
        other.workspacePath == workspacePath &&
        other.relativePath == relativePath;
  }

  @override
  int get hashCode => Object.hash(workspacePath, relativePath);
}

class const EditorSessionHandle({
  required final bool Function() isDirty,
  required final Future<void> Function() save,
  required final Future<void> Function() discard,
  final String Function()? snapshotText,
  final void Function(WorkspaceEditorRevealTarget target)? reveal,
  final Future<void> Function()? reload,
});

class const WorkspaceEditorRevealTarget({
  required final int line,
  required final int column,
  required final int matchLength,
});

class EditorDocumentSession({final VoidCallback? _onChanged}) {
  String? workspacePath;
  String? relativePath;
  String? loadedRawText;
  String? loadedText;
  String? currentText;
  String? contentToken;
  Object? loadError;
  WorkspaceEditorRevealTarget? pendingReveal;
  native.WorkspaceTextEncoding? requestedEncoding;
  native.WorkspaceTextEncoding? encoding;
  int tabSize = 4;
  bool nativeBacked = false;
  int loadedDocumentVersion = 0;
  int currentDocumentVersion = 0;

  bool get hasSnapshot =>
      nativeBacked || currentText != null || loadError != null;

  bool get canSave =>
      (nativeBacked || loadedText != null) &&
      loadError == null &&
      encoding != null;

  bool get isDirty => nativeBacked
      ? currentDocumentVersion != loadedDocumentVersion
      : loadedText != null && currentText != loadedText;

  void attachFile({
    required String workspacePath,
    required String relativePath,
  }) {
    if (this.workspacePath == workspacePath &&
        this.relativePath == relativePath) {
      return;
    }
    this.workspacePath = workspacePath;
    this.relativePath = relativePath;
    _notifyChanged();
  }

  void acceptLoaded(
    native.WorkspaceEditorTextFile file, {
    int tabSize = 4,
    native.WorkspaceTextEncoding? requestedEncoding,
  }) {
    this.tabSize = tabSize;
    this.requestedEncoding = requestedEncoding;
    encoding = file.encoding;
    loadedRawText = file.rawContent;
    loadedText = file.displayContent;
    currentText = loadedText;
    contentToken = file.contentToken;
    loadError = null;
    nativeBacked = false;
    loadedDocumentVersion = 0;
    currentDocumentVersion = 0;
    _notifyChanged();
  }

  void acceptNativeLoaded({
    required native.WorkspaceTextEncoding encoding,
    required String contentToken,
    required int documentVersion,
    int tabSize = 4,
    native.WorkspaceTextEncoding? requestedEncoding,
  }) {
    this.tabSize = tabSize;
    this.requestedEncoding = requestedEncoding;
    this.encoding = encoding;
    loadedRawText = null;
    loadedText = null;
    currentText = null;
    this.contentToken = contentToken;
    nativeBacked = true;
    loadedDocumentVersion = documentVersion;
    currentDocumentVersion = documentVersion;
    loadError = null;
    _notifyChanged();
  }

  void acceptSaved(native.WorkspaceEditorTextFile file, {int? tabSize}) {
    acceptLoaded(
      file,
      tabSize: tabSize ?? this.tabSize,
      requestedEncoding: requestedEncoding,
    );
  }

  void acceptNativeSaved({
    required native.WorkspaceTextEncoding encoding,
    required String contentToken,
    required int savedDocumentVersion,
    required int currentDocumentVersion,
    int? tabSize,
  }) {
    this.tabSize = tabSize ?? this.tabSize;
    this.encoding = encoding;
    loadedRawText = null;
    loadedText = null;
    currentText = null;
    this.contentToken = contentToken;
    nativeBacked = true;
    loadedDocumentVersion = savedDocumentVersion;
    this.currentDocumentVersion = currentDocumentVersion;
    loadError = null;
    _notifyChanged();
  }

  void acceptLoadError(Object error) {
    loadedRawText = null;
    loadedText = null;
    currentText = null;
    contentToken = null;
    encoding = null;
    nativeBacked = false;
    loadedDocumentVersion = 0;
    currentDocumentVersion = 0;
    loadError = error;
    _notifyChanged();
  }

  void clearSnapshot() {
    loadedRawText = null;
    loadedText = null;
    currentText = null;
    contentToken = null;
    requestedEncoding = null;
    encoding = null;
    nativeBacked = false;
    loadedDocumentVersion = 0;
    currentDocumentVersion = 0;
    loadError = null;
    _notifyChanged();
  }

  bool updateCurrentText(String text) {
    if (currentText == text) {
      return false;
    }
    currentText = text;
    _notifyChanged();
    return true;
  }

  bool updateNativeDocumentVersion(int version) {
    if (!nativeBacked || currentDocumentVersion == version) {
      return false;
    }
    currentDocumentVersion = version;
    _notifyChanged();
    return true;
  }

  void _notifyChanged() {
    _onChanged?.call();
  }
}
