part of 'workspace_git_diff_surface.dart';

extension _WorkspaceGitDiffSurfaceLoading on _WorkspaceGitDiffSurfaceState {
  void _load() {
    final loadGeneration = ++_diffLoadGeneration;
    final readingDiffCompletion = _readingDiffCompletion;
    if (readingDiffCompletion != null && !readingDiffCompletion.isCompleted) {
      _cancelReadingDiff();
      unawaited(
        readingDiffCompletion.future.then((_) {
          if (mounted && loadGeneration == _diffLoadGeneration) {
            _loadNow();
          }
        }),
      );
      return;
    }
    _loadNow();
  }

  void _loadNow() {
    _readingDiffGeneration += 1;
    final backend = ref.read(gitBackendProvider);
    final scope = widget.tab.gitDiffScope;
    final filePath = widget.tab.filePath;
    final sourceControlScope = _sourceControlScope;
    final sourceFilePath = sourceControlScope.toSourceRelativePath(filePath);
    final sourceOldPath = sourceControlScope.toSourceRelativePath(
      widget.tab.gitDiffOldPath,
    );
    final area = widget.tab.gitDiffArea;
    final nextFuture = _isCommitBackedDiff
        ? _loadCommitDiff(
            backend: backend,
            sourceControlScope: sourceControlScope,
            sourceFilePath: sourceFilePath,
            sourceOldPath: sourceOldPath,
          )
        : switch (scope) {
            WorkspaceGitDiffScope.all => backend.diffAll(
              path: sourceControlScope.path,
            ),
            WorkspaceGitDiffScope.fileAll =>
              sourceFilePath == null
                  ? Future<GitDiffResult>.value(const GitDiffResult(files: []))
                  : backend.diffAll(
                      path: sourceControlScope.path,
                      filePath: sourceFilePath,
                    ),
            WorkspaceGitDiffScope.file =>
              sourceFilePath == null || area == null
                  ? Future<GitDiffResult>.value(const GitDiffResult(files: []))
                  : backend.diff(
                      path: sourceControlScope.path,
                      filePath: sourceFilePath,
                      area: area,
                    ),
            null => Future<GitDiffResult>.value(const GitDiffResult(files: [])),
          };
    _updateDiffState(() {
      _loadedResult = null;
      _fullFileBytes = const <GitDiffFile, Uint8List>{};
      _readingDiffResult = null;
      _readingDiffOriginalSnapshot = null;
      _showReadingDiff = false;
      _readingDiffBusy = false;
      _readingDiffProgress = null;
      _readingDiffError = null;
      _readingDiffAgentLabel = null;
      _readingDiffModel = null;
      _future = nextFuture;
    });
    unawaited(
      nextFuture.then(
        (result) async {
          if (!mounted || _future != nextFuture) {
            return;
          }
          _updateDiffState(() {
            _loadedResult = result;
          });
          final fullFileBytes = await _loadFullFileBytes(
            backend: backend,
            sourceControlScope: sourceControlScope,
            result: result,
          );
          if (!mounted || _future != nextFuture) {
            return;
          }
          _updateDiffState(() {
            _fullFileBytes = fullFileBytes;
          });
        },
        onError: (_) {
          if (!mounted || _future != nextFuture) {
            return;
          }
          _updateDiffState(() {
            _loadedResult = null;
          });
        },
      ),
    );
  }

  Future<Map<GitDiffFile, Uint8List>> _loadFullFileBytes({
    required GitBackend backend,
    required WorkspaceSourceControlScope sourceControlScope,
    required GitDiffResult result,
  }) async {
    final entries = await Future.wait(
      result.files.map((file) async {
        if (file.isBinary || file.isLarge || file.isGitlink) {
          return null;
        }
        final oldSide = file.status == GitChangeStatus.deleted;
        try {
          final bytes = await backend.diffBlobBytes(
            path: sourceControlScope.path,
            filePath: file.path,
            oldPath: file.oldPath,
            area: _isCommitBackedDiff ? null : file.area,
            commitOid: _isCommitBackedDiff ? widget.tab.gitDiffCommitOid : null,
            parentOid: _isCommitBackedDiff ? widget.tab.gitDiffParentOid : null,
            oldSide: oldSide,
          );
          return bytes == null ? null : MapEntry(file, bytes);
        } catch (_) {
          return null;
        }
      }),
    );
    return <GitDiffFile, Uint8List>{
      for (final entry in entries)
        if (entry != null) entry.key: entry.value,
    };
  }
}
