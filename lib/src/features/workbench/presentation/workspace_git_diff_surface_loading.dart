part of 'workspace_git_diff_surface.dart';

const _maxProgressiveDiffPreviewBytes = 512 * 1024;
const _maxEditableFullFileBytes = 512 * 1024;
const _maxFullFileRenderBytes = 2 * 1024 * 1024;
const _progressiveFirstDiffPageSize = 1;
const _progressiveDiffPageSize = 4;

class _ProgressiveDiffPage {
  const _ProgressiveDiffPage({required this.result, required this.nextIndex});

  final GitDiffResult result;
  final int? nextIndex;
}

extension _WorkspaceGitDiffSurfaceLoading on _WorkspaceGitDiffSurfaceState {
  GitDiffContentMode get _contentModeForLoading {
    final override = _overrideContentMode;
    if (override != null) return override;
    try {
      return ref.read(workbenchControllerProvider).viewPrefs.gitDiffContentMode;
    } catch (_) {
      return GitDiffContentMode.fullFile;
    }
  }

  void _load({
    bool preserveEditableDocuments = false,
    bool preserveReadingDiff = false,
  }) {
    final loadGeneration = ++_diffLoadGeneration;
    final readingDiffCompletion = _readingDiffCompletion;
    if (!preserveReadingDiff &&
        readingDiffCompletion != null &&
        !readingDiffCompletion.isCompleted) {
      _cancelReadingDiff();
      unawaited(
        readingDiffCompletion.future.then((_) {
          if (mounted && loadGeneration == _diffLoadGeneration) {
            _loadNow(
              loadGeneration,
              preserveEditableDocuments: preserveEditableDocuments,
              preserveReadingDiff: false,
            );
          }
        }),
      );
      return;
    }
    _loadNow(
      loadGeneration,
      preserveEditableDocuments: preserveEditableDocuments,
      preserveReadingDiff: preserveReadingDiff,
    );
  }

  void _loadNow(
    int loadGeneration, {
    required bool preserveEditableDocuments,
    required bool preserveReadingDiff,
  }) {
    if (!preserveReadingDiff) {
      _readingDiffGeneration += 1;
    }
    final backend = ref.read(gitBackendProvider);
    final scope = widget.tab.gitDiffScope;
    final filePath = widget.tab.filePath;
    final sourceControlScope = _sourceControlScope;
    final sourceFilePath = sourceControlScope.toSourceRelativePath(filePath);
    final sourceOldPath = sourceControlScope.toSourceRelativePath(
      widget.tab.gitDiffOldPath,
    );
    final area = widget.tab.gitDiffArea;
    final progressiveAllPathsFuture =
        !_isCommitBackedDiff && scope == WorkspaceGitDiffScope.all
        ? _loadProgressiveAllPaths(
            backend: backend,
            sourceControlPath: sourceControlScope.path,
          )
        : null;
    final progressiveFirstPageFuture = progressiveAllPathsFuture == null
        ? null
        : _loadFirstProgressiveAllPage(
            backend: backend,
            sourceControlPath: sourceControlScope.path,
            pathsFuture: progressiveAllPathsFuture,
          );
    final nextFuture = _isCommitBackedDiff
        ? _loadCommitDiff(
            backend: backend,
            sourceControlScope: sourceControlScope,
            sourceFilePath: sourceFilePath,
            sourceOldPath: sourceOldPath,
          )
        : switch (scope) {
            WorkspaceGitDiffScope.all => progressiveFirstPageFuture!.then(
              (page) => page.result,
            ),
            WorkspaceGitDiffScope.fileAll =>
              sourceFilePath == null
                  ? Future<GitDiffResult>.value(const GitDiffResult(files: []))
                  : backend.diffAll(
                      path: sourceControlScope.path,
                      filePath: sourceFilePath,
                      whitespaceMode: _whitespaceMode,
                    ),
            WorkspaceGitDiffScope.file =>
              sourceFilePath == null || area == null
                  ? Future<GitDiffResult>.value(const GitDiffResult(files: []))
                  : backend.diff(
                      path: sourceControlScope.path,
                      filePath: sourceFilePath,
                      area: area,
                      whitespaceMode: _whitespaceMode,
                    ),
            null => Future<GitDiffResult>.value(const GitDiffResult(files: [])),
          };
    _updateDiffState(() {
      _loadedResult = null;
      _fullFileContents = const <GitDiffFile, _FullFileContents>{};
      _fullFilePreviewLimitedPaths.clear();
      if (!preserveEditableDocuments) {
        _editableDocuments.clear();
      }
      if (!preserveReadingDiff) {
        _readingDiffResult = null;
        _readingDiffOriginalSnapshot = null;
        _showReadingDiff = false;
        _readingDiffBusy = false;
        _readingDiffProgress = null;
        _readingDiffError = null;
        _readingDiffAgentLabel = null;
        _readingDiffModel = null;
      }
      _future = nextFuture;
    });
    unawaited(
      nextFuture.then(
        (result) {
          if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
            return;
          }
          _updateDiffState(() {
            _loadedResult = result;
          });
          final allPathsFuture = progressiveAllPathsFuture;
          final firstPageFuture = progressiveFirstPageFuture;
          if (allPathsFuture != null && firstPageFuture != null) {
            unawaited(
              firstPageFuture.then(
                (firstPage) => _loadRemainingProgressiveAllDiff(
                  backend: backend,
                  sourceControlPath: sourceControlScope.path,
                  pathsFuture: allPathsFuture,
                  firstPage: firstPage,
                  nextFuture: nextFuture,
                  loadGeneration: loadGeneration,
                ),
              ),
            );
          }
          unawaited(
            _loadFullFileContents(
              backend: backend,
              sourceControlScope: sourceControlScope,
              result: result,
              nextFuture: nextFuture,
              loadGeneration: loadGeneration,
            ),
          );
        },
        onError: (_) {
          if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
            return;
          }
          _updateDiffState(() {
            _loadedResult = null;
          });
        },
      ),
    );
  }

  Future<List<String>?> _loadProgressiveAllPaths({
    required GitBackend backend,
    required String sourceControlPath,
  }) async {
    final status = await backend.status(sourceControlPath);
    final hasNestedSubmoduleChanges = status.entries.any((entry) {
      final submodule = entry.submodule;
      return submodule != null &&
          (submodule.trackedChanges || submodule.untrackedChanges);
    });
    if (hasNestedSubmoduleChanges) {
      // The combined Rust operation includes nested submodule worktree files;
      // keep that path as one operation until it has a paged equivalent.
      return null;
    }
    final paths = <String>{};
    for (final entry in status.entries) {
      paths.add(entry.path);
    }
    return paths.toList(growable: false);
  }

  Future<_ProgressiveDiffPage> _loadFirstProgressiveAllPage({
    required GitBackend backend,
    required String sourceControlPath,
    required Future<List<String>?> pathsFuture,
  }) async {
    final paths = await pathsFuture;
    if (paths == null) {
      return _ProgressiveDiffPage(
        result: await backend.diffAll(
          path: sourceControlPath,
          whitespaceMode: _whitespaceMode,
        ),
        nextIndex: null,
      );
    }
    if (paths.isEmpty) {
      return const _ProgressiveDiffPage(
        result: GitDiffResult(files: []),
        nextIndex: null,
      );
    }
    final page = await backend.diffAllPage(
      path: sourceControlPath,
      filePaths: paths
          .take(_progressiveFirstDiffPageSize)
          .toList(growable: false),
      whitespaceMode: _whitespaceMode,
    );
    return _ProgressiveDiffPage(
      result: GitDiffResult(files: page.files, truncated: page.truncated),
      nextIndex: paths.length > _progressiveFirstDiffPageSize
          ? _progressiveFirstDiffPageSize
          : null,
    );
  }

  Future<void> _loadRemainingProgressiveAllDiff({
    required GitBackend backend,
    required String sourceControlPath,
    required Future<List<String>?> pathsFuture,
    required _ProgressiveDiffPage firstPage,
    required Future<GitDiffResult> nextFuture,
    required int loadGeneration,
  }) async {
    final paths = await pathsFuture;
    if (paths == null || firstPage.nextIndex == null) {
      return;
    }
    var accumulated = firstPage.result;
    var accumulatedBytes = _diffPreviewBytes(firstPage.result);
    if (accumulatedBytes >= _maxProgressiveDiffPreviewBytes) {
      return;
    }
    var nextIndex = firstPage.nextIndex;
    while (nextIndex != null && nextIndex < paths.length) {
      if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
        return;
      }
      final pageStartIndex = nextIndex;
      final pagePaths = paths
          .skip(pageStartIndex)
          .take(_progressiveDiffPageSize)
          .toList(growable: false);
      if (pagePaths.isEmpty) {
        return;
      }
      final page = await _loadProgressiveDiffPage(
        backend: backend,
        sourceControlPath: sourceControlPath,
        filePaths: pagePaths,
      );
      if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
        return;
      }
      if (page == null) {
        return;
      }
      final result = GitDiffResult(
        files: page.files,
        truncated: page.truncated,
      );
      if (result.files.isNotEmpty) {
        final resultBytes = _diffPreviewBytes(result);
        if (resultBytes > _maxProgressiveDiffPreviewBytes - accumulatedBytes) {
          _updateDiffState(() {
            _loadedResult = GitDiffResult(
              files: accumulated.files,
              truncated: true,
            );
          });
          return;
        }
        accumulated = GitDiffResult(
          files: <GitDiffFile>[...accumulated.files, ...result.files],
          truncated: accumulated.truncated || result.truncated,
        );
        accumulatedBytes += resultBytes;
        _updateDiffState(() {
          _loadedResult = accumulated;
        });
        unawaited(
          _loadFullFileContents(
            backend: backend,
            sourceControlScope: _sourceControlScope,
            result: result,
            nextFuture: nextFuture,
            loadGeneration: loadGeneration,
          ),
        );
      }
      nextIndex = pageStartIndex + pagePaths.length;
    }
  }

  Future<GitDiffPage?> _loadProgressiveDiffPage({
    required GitBackend backend,
    required String sourceControlPath,
    required List<String> filePaths,
  }) async {
    try {
      return await backend.diffAllPage(
        path: sourceControlPath,
        filePaths: filePaths,
        whitespaceMode: _whitespaceMode,
      );
    } catch (_) {
      // A later page is best-effort once the first page is visible. Keep the
      // already-rendered pages usable if a file changes or disappears.
      return null;
    }
  }

  int _diffPreviewBytes(GitDiffResult result) {
    var total = 0;
    for (final file in result.files) {
      for (final line in file.lines) {
        total += line.text.length + 1;
      }
    }
    return total;
  }

  bool _isCurrentDiffLoad(
    Future<GitDiffResult> nextFuture,
    int loadGeneration,
  ) {
    return mounted &&
        _future == nextFuture &&
        _diffLoadGeneration == loadGeneration;
  }

  Future<void> _loadFullFileContents({
    required GitBackend backend,
    required WorkspaceSourceControlScope sourceControlScope,
    required GitDiffResult result,
    required Future<GitDiffResult> nextFuture,
    required int loadGeneration,
  }) async {
    if (_contentModeForLoading != GitDiffContentMode.fullFile) {
      return;
    }
    Future<Uint8List?> loadSide(GitDiffFile file, bool oldSide) async {
      try {
        return await backend.diffBlobBytes(
          path: sourceControlScope.path,
          filePath: file.path,
          oldPath: file.oldPath,
          area: _isCommitBackedDiff ? null : file.area,
          commitOid: _isCommitBackedDiff ? widget.tab.gitDiffCommitOid : null,
          parentOid: _isCommitBackedDiff ? widget.tab.gitDiffParentOid : null,
          oldSide: oldSide,
        );
      } catch (_) {
        return null;
      }
    }

    // Show the diff preview first, then hydrate one file at a time. Loading
    // both blobs for every changed file in parallel can create a large native
    // read burst and keeps the first frame waiting for unrelated files.
    for (final file in result.files) {
      if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
        return;
      }
      if (file.isBinary || file.isLarge || file.isGitlink) {
        continue;
      }
      final loadOld =
          file.status != GitChangeStatus.added &&
          file.status != GitChangeStatus.untracked;
      final loadNew = file.status != GitChangeStatus.deleted;
      final sides = await Future.wait(<Future<Uint8List?>>[
        if (loadOld) loadSide(file, true) else Future.value(null),
        if (loadNew) loadSide(file, false) else Future.value(null),
      ]);
      if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
        return;
      }
      final largestSideBytes = sides.fold<int>(0, (largest, bytes) {
        final length = bytes?.length ?? 0;
        return math.max(largest, length);
      });
      if (largestSideBytes > _maxFullFileRenderBytes) {
        _updateDiffState(() {
          _fullFilePreviewLimitedPaths.add(file.path);
          _editableDocuments.remove(file.path);
        });
        continue;
      }
      if (largestSideBytes <= _maxEditableFullFileBytes) {
        await _ensureEditableDocument(
          file: file,
          sourceControlScope: sourceControlScope,
          loadGeneration: loadGeneration,
        );
        if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
          return;
        }
      } else {
        _updateDiffState(() => _editableDocuments.remove(file.path));
      }
      late _FullFileContents contents;
      while (true) {
        final encodingGeneration = _encodingGeneration;
        final encoding = _encodingSelection.encoding;
        contents = await _decodeFullFileContents(
          oldBytes: sides[0],
          newBytes: sides[1],
          encoding: encoding,
        );
        if (!_isCurrentDiffLoad(nextFuture, loadGeneration)) {
          return;
        }
        if (encodingGeneration == _encodingGeneration) {
          break;
        }
      }
      if (contents.oldBytes == null && contents.newBytes == null) {
        continue;
      }
      _updateDiffState(() {
        _fullFileContents = <GitDiffFile, _FullFileContents>{
          ..._fullFileContents,
          file: contents,
        };
      });
    }
  }

  Future<_FullFileContents> _decodeFullFileContents({
    required Uint8List? oldBytes,
    required Uint8List? newBytes,
    required native.WorkspaceTextEncoding? encoding,
  }) async {
    Future<native.WorkspaceDecodedText?> decode(Uint8List? bytes) async {
      if (bytes == null) return null;
      try {
        return await ref
            .read(workspaceFileServiceProvider)
            .decodeTextBytes(bytes: bytes, encoding: encoding);
      } catch (_) {
        return null;
      }
    }

    final decoded = await Future.wait<native.WorkspaceDecodedText?>(
      <Future<native.WorkspaceDecodedText?>>[
        decode(oldBytes),
        decode(newBytes),
      ],
    );
    return _FullFileContents(
      oldBytes: oldBytes,
      newBytes: newBytes,
      oldDecoded: decoded[0],
      newDecoded: decoded[1],
    );
  }

  Future<void> _changeDiffEncoding(
    WorkspaceTextEncodingSelection selection,
  ) async {
    if (_encodingSelection == selection) return;
    final generation = ++_encodingGeneration;
    final snapshot = _fullFileContents;
    _updateDiffState(() => _encodingSelection = selection);
    final decoded = <GitDiffFile, _FullFileContents>{};
    for (final entry in snapshot.entries) {
      decoded[entry.key] = await _decodeFullFileContents(
        oldBytes: entry.value.oldBytes,
        newBytes: entry.value.newBytes,
        encoding: selection.encoding,
      );
      if (!mounted || generation != _encodingGeneration) return;
    }
    if (!mounted || generation != _encodingGeneration) return;
    _updateDiffState(() {
      _fullFileContents = <GitDiffFile, _FullFileContents>{
        ..._fullFileContents,
        ...decoded,
      };
    });
  }

  native.WorkspaceTextEncoding? get _detectedDiffEncoding {
    native.WorkspaceTextEncoding? detected;
    for (final contents in _fullFileContents.values) {
      for (final encoding in <native.WorkspaceTextEncoding?>[
        contents.oldDecoded?.encoding,
        contents.newDecoded?.encoding,
      ]) {
        if (encoding == null) continue;
        if (detected == null) {
          detected = encoding;
        } else if (detected != encoding) {
          return null;
        }
      }
    }
    return detected;
  }
}
