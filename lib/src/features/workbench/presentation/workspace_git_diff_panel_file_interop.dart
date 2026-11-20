part of 'workspace_git_diff_panel.dart';

class _GitSourceClipboard {
  const _GitSourceClipboard({
    required this.relativePaths,
    required this.cut,
    required this.root,
    this.sequenceNumber,
  });

  final List<String> relativePaths;
  final bool cut;
  final String root;
  final int? sequenceNumber;
}

extension _WorkspaceGitDiffPanelFileInterop on _WorkspaceGitDiffPanelState {
  List<GitSourceFileItem> _gitVisibleFileItems() {
    final current = ref
        .read(
          workspaceSourceControlControllerProvider(
            widget.sourceControlScope.path,
          ),
        )
        .asData
        ?.value;
    if (current == null) return const <GitSourceFileItem>[];
    return visibleGitSourceFileItems(
      groups: _groupsFor(_filteredStatus(current.status)),
      viewMode: widget.viewMode,
      collapsedSections: _collapsedSections,
      collapsedTreeNodes: _collapsedTreeNodes,
    );
  }

  GlobalKey _gitDropRowKey(String id) =>
      _gitFileDropRowKeys.putIfAbsent(id, () => GlobalKey());

  void _selectGitDirectory(
    String id,
    List<String> visibleIds, {
    required bool contextMenu,
  }) {
    final keyboard = HardwareKeyboard.instance;
    _setPanelState(() {
      if (contextMenu) {
        _changeSelection.contextSelect(id);
      } else {
        _changeSelection.tap(
          id,
          visibleIds: visibleIds,
          shift: keyboard.isShiftPressed,
          toggle: keyboard.isControlPressed || keyboard.isMetaPressed,
        );
      }
    });
  }

  List<String> _gitSelectedFilePaths({String? clickedId}) {
    return gitSourceSelectedPaths(
      visible: _gitVisibleFileItems(),
      selectedIds: _changeSelection.ids,
      clickedId: clickedId,
    );
  }

  String _gitDropDirectoryAt(Offset position) {
    final visible = _gitVisibleFileItems();
    for (final item in visible) {
      final render = _gitFileDropRowKeys[item.id]?.currentContext
          ?.findRenderObject();
      if (render is! RenderBox || !render.attached || !render.hasSize) {
        continue;
      }
      if (!render.size.contains(render.globalToLocal(position))) {
        continue;
      }
      return item.isDirectory
          ? item.relativePath
          : _gitRelativeParent(item.relativePath);
    }
    return '';
  }

  String _gitRelativeParent(String path) {
    final index = path.lastIndexOf('/');
    return index < 0 ? '' : path.substring(0, index);
  }

  String _gitPasteTarget(String? targetId) {
    final visible = _gitVisibleFileItems();
    final id = targetId ?? _changeSelection.anchor;
    for (final item in visible) {
      if (item.id == id) {
        return item.isDirectory
            ? item.relativePath
            : _gitRelativeParent(item.relativePath);
      }
    }
    // An explicit row context menu must not silently paste elsewhere after
    // a filter/collapse change; keyboard paste defaults to the repository root.
    if (targetId != null) {
      throw const FileSystemException('Git drop target is no longer visible');
    }
    return '';
  }

  void _gitFileMessage(String message, {bool error = false}) {
    if (!mounted) return;
    AleraToast.show(
      context,
      message: message,
      tone: error ? AleraToastTone.error : AleraToastTone.success,
    );
  }

  Future<void> _gitCopyOrCut({String? clickedId, required bool cut}) async {
    if (_gitFileTransferBusy) return;
    final scope = widget.sourceControlScope.path;
    final selected = _gitSelectedFilePaths(clickedId: clickedId);
    try {
      final checked = await _gitFileGuard.checkedSourcePaths(
        workspacePath: scope,
        relativePaths: selected,
        limit: gitSourceClipboardLimit,
      );
      if (!mounted || scope != widget.sourceControlScope.path) return;
      if (cut &&
          _gitEditorSessions.hasOpenFilesUnder(
            workspacePath: scope,
            relativePaths: selected,
          )) {
        throw const FileSystemException(
          'Close editor tabs under the selected files/folders before Cut',
        );
      }
      int? sequence;
      if (Platform.isWindows) {
        sequence = await _gitFileService.writeSystemFileClipboard(
          paths: <String>[
            for (final source in checked) userVisibleWorkspacePath(source),
          ],
          operation: cut
              ? WorkspaceFileClipboardOperation.cut
              : WorkspaceFileClipboardOperation.copy,
        );
      }
      if (!mounted || scope != widget.sourceControlScope.path) return;
      _gitFileClipboard = _GitSourceClipboard(
        relativePaths: List<String>.unmodifiable(selected),
        cut: cut,
        root: scope,
        sequenceNumber: sequence,
      );
      _gitFileMessage(
        '${cut ? 'Cut' : 'Copied'} ${selected.length} Git worktree item(s)',
      );
    } catch (error) {
      _gitFileMessage('Git file Copy/Cut refused: $error', error: true);
    }
  }

  Future<void> _gitPaste({String? targetId}) async {
    if (_gitFileTransferBusy) return;
    final scope = widget.sourceControlScope.path;
    _gitFileTransferBusy = true;
    try {
      final target = _gitPasteTarget(targetId);
      final targetAbsolute = await _gitFileGuard.checkedDirectory(
        workspacePath: scope,
        relativeDirectory: target,
      );
      final local = _gitFileClipboard;
      if (Platform.isWindows) {
        final native = await _gitFileService.readSystemFileClipboard();
        if (native != null && !_gitIsOurNativeClipboard(native)) {
          // Inbound OS Cut is imported as a COPY: never delete an external
          // file merely because it was marked Cut by another application.
          await _gitImportExternal(native.paths, target, fromClipboard: true);
          return;
        }
        if (local?.sequenceNumber case final sequence?) {
          if (native == null ||
              await _gitFileService.systemFileClipboardSequenceNumber() !=
                  sequence) {
            throw const FileSystemException('File clipboard was replaced');
          }
        }
      }
      if (local == null || local.root != scope) {
        throw const FileSystemException(
          'Select Git files/folders to Copy or Cut first',
        );
      }
      final checked = await _gitFileGuard.checkedSourcePaths(
        workspacePath: scope,
        relativePaths: local.relativePaths,
        limit: gitSourceClipboardLimit,
      );
      await _gitFileGuard.preflightDestination(
        absoluteSources: checked,
        destinationFolder: targetAbsolute,
      );
      if (local.cut &&
          _gitEditorSessions.hasOpenFilesUnder(
            workspacePath: scope,
            relativePaths: local.relativePaths,
          )) {
        throw const FileSystemException(
          'Close open Editor tabs before moving these items',
        );
      }
      for (final path in local.relativePaths) {
        if (local.cut) {
          await _gitFileService.moveEntry(
            workspacePath: scope,
            relativePath: path,
            targetParentRelativePath: target,
          );
        } else {
          await _gitFileService.copyEntry(
            workspacePath: scope,
            relativePath: path,
            targetParentRelativePath: target,
          );
        }
      }
      if (!mounted || scope != widget.sourceControlScope.path) return;
      if (local.cut) {
        _gitFileClipboard = null;
        if (local.sequenceNumber case final sequence?) {
          await _gitFileService.clearSystemFileClipboardIfSequence(sequence);
        }
      }
      _gitFileMessage('Pasted ${local.relativePaths.length} worktree item(s)');
    } catch (error) {
      _gitFileMessage('Git Paste refused/stopped: $error', error: true);
    } finally {
      _gitFileTransferBusy = false;
      if (mounted && scope == widget.sourceControlScope.path) {
        await _gitRefreshAfterFileTransfer();
      }
    }
  }

  bool _gitIsOurNativeClipboard(WorkspaceFileClipboardPayload payload) {
    final local = _gitFileClipboard;
    if (local == null ||
        local.root != widget.sourceControlScope.path ||
        local.sequenceNumber != payload.sequenceNumber ||
        local.cut !=
            (payload.operation == WorkspaceFileClipboardOperation.cut) ||
        local.relativePaths.length != payload.paths.length) {
      return false;
    }
    for (var index = 0; index < payload.paths.length; index++) {
      final localPath = p.normalize(
        p.joinAll(<String>[
          local.root,
          ...local.relativePaths[index].split('/'),
        ]),
      );
      if (!p.equals(
        p.normalize(userVisibleWorkspacePath(payload.paths[index])),
        p.normalize(userVisibleWorkspacePath(localPath)),
      ))
        return false;
    }
    return true;
  }

  Future<void> _gitImportExternal(
    List<String> sources,
    String targetRelative, {
    bool fromClipboard = false,
  }) async {
    if (!mounted ||
        _gitExternalImportActive ||
        (_gitFileTransferBusy && !fromClipboard)) {
      return;
    }
    _gitExternalImportActive = true;
    final scope = widget.sourceControlScope.path;
    if (sources.isEmpty || sources.length > gitSourceExternalDropLimit) {
      _gitFileMessage(
        'Drop 1-${gitSourceExternalDropLimit} files/folders at once',
        error: true,
      );
      _gitExternalImportActive = false;
      return;
    }
    try {
      final folder = await _gitFileGuard.checkedDirectory(
        workspacePath: scope,
        relativeDirectory: targetRelative,
      );
      // Reject predictable basename collisions before importing the first
      // item; Rust importWorkspaceEntries enforces source safety and bounds.
      await _gitFileGuard.preflightDestination(
        absoluteSources: sources,
        destinationFolder: folder,
      );
      await _gitFileService.importEntries(
        workspacePath: scope,
        sourcePaths: sources,
        targetParentRelativePath: targetRelative,
        moveSources: false,
      );
      if (mounted && scope == widget.sourceControlScope.path) {
        _gitFileMessage('Imported ${sources.length} worktree item(s)');
      }
    } catch (error) {
      _gitFileMessage('Git folder import refused/stopped: $error', error: true);
    } finally {
      _gitExternalImportActive = false;
      if (mounted && scope == widget.sourceControlScope.path) {
        await _gitRefreshAfterFileTransfer();
      }
    }
  }

  Future<void> _gitMoveInternalDrag(
    List<String> sourcePaths,
    String targetRelative,
  ) async {
    if (_gitFileTransferBusy || sourcePaths.isEmpty) return;
    final scope = widget.sourceControlScope.path;
    _gitFileTransferBusy = true;
    try {
      if (sourcePaths.length > gitSourceClipboardLimit) {
        throw const FileSystemException(
          'Too many items for an in-app folder move',
        );
      }
      final root = await Directory(scope).resolveSymbolicLinks();
      final relative = <String>[];
      for (final source in sourcePaths) {
        final normalized = p.normalize(source);
        if (!p.isWithin(root, normalized)) {
          throw FileSystemException(
            'Drag source is not in the active Git worktree',
            source,
          );
        }
        relative.add(p.relative(normalized, from: root).replaceAll('\\', '/'));
      }
      final unique = <String>{...relative}.toList()..sort();
      final topLevel = <String>[];
      for (final path in unique) {
        if (!topLevel.any((parent) => path.startsWith('$parent/'))) {
          topLevel.add(path);
        }
      }
      final checked = await _gitFileGuard.checkedSourcePaths(
        workspacePath: scope,
        relativePaths: topLevel,
        limit: gitSourceClipboardLimit,
      );
      final folder = await _gitFileGuard.checkedDirectory(
        workspacePath: scope,
        relativeDirectory: targetRelative,
      );
      await _gitFileGuard.preflightDestination(
        absoluteSources: checked,
        destinationFolder: folder,
      );
      if (_gitEditorSessions.hasOpenFilesUnder(
        workspacePath: scope,
        relativePaths: topLevel,
      )) {
        throw const FileSystemException(
          'Close open Editor tabs before moving dragged files',
        );
      }
      for (final path in topLevel) {
        await _gitFileService.moveEntry(
          workspacePath: scope,
          relativePath: path,
          targetParentRelativePath: targetRelative,
        );
      }
      if (mounted && scope == widget.sourceControlScope.path) {
        _gitFileMessage('Moved ${topLevel.length} worktree item(s)');
      }
    } catch (error) {
      _gitFileMessage('Git drag refused/stopped: $error', error: true);
    } finally {
      _gitFileTransferBusy = false;
      if (mounted && scope == widget.sourceControlScope.path) {
        await _gitRefreshAfterFileTransfer();
      }
    }
  }

  void _gitMaybeStartNativeDrag(String id, DragUpdateDetails details) {
    if (!Platform.isWindows ||
        !mounted ||
        _gitNativeDragBusy ||
        _gitNativeDragHandedOff) {
      return;
    }
    _gitNativeDragBusy = true;
    unawaited(_gitStartNativeDrag(id, details));
  }

  Future<void> _gitStartNativeDrag(String id, DragUpdateDetails details) async {
    final scope = widget.sourceControlScope.path;
    try {
      final view = View.of(context);
      final width = view.physicalSize.width / view.devicePixelRatio;
      final height = view.physicalSize.height / view.devicePixelRatio;
      final pointer = details.globalPosition;
      if (pointer.dx >= 0 &&
          pointer.dy >= 0 &&
          pointer.dx < width &&
          pointer.dy < height &&
          !await _gitNativeDrag.cursorOutsideWindow()) {
        return;
      }
      if (!mounted || _gitNativeDragHandedOff) return;
      _gitNativeDragHandedOff = true;
      final relative = _gitSelectedFilePaths(clickedId: id);
      final paths = await _gitFileGuard.checkedSourcePaths(
        workspacePath: scope,
        relativePaths: relative,
        limit: gitSourceDragLimit,
      );
      if (_gitEditorSessions.hasOpenFilesUnder(
        workspacePath: scope,
        relativePaths: relative,
      )) {
        throw const FileSystemException(
          'Close open editor tabs before external drag',
        );
      }
      if (!mounted || scope != widget.sourceControlScope.path) return;
      final result = await _gitNativeDrag.startDrag(<String>[
        for (final path in paths) userVisibleWorkspacePath(path),
      ]);
      if (result == WindowsFileDragEffect.move &&
          mounted &&
          scope == widget.sourceControlScope.path) {
        await _gitRefreshAfterFileTransfer();
      }
    } on MissingPluginException {
      // Older portable builds may not include the native FileDrag channel.
    } catch (error) {
      _gitFileMessage('Native Git file drag refused: $error', error: true);
    } finally {
      _gitNativeDragBusy = false;
    }
  }

  Future<void> _gitRefreshAfterFileTransfer() async {
    _gitFileService.scheduleQuickOpenRefresh(
      workspacePath: widget.sourceControlScope.path,
      debounce: Duration.zero,
    );
    try {
      await _notifier.refresh();
    } catch (_) {
      // Do not conceal the previous transfer outcome if a watcher is late.
    }
  }
}
