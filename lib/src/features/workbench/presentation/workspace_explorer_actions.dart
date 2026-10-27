part of 'workspace_explorer.dart';

extension _WorkspaceExplorerActions on _WorkspaceExplorerState {
  String _pasteTargetDirectory() {
    final selected = _selectedVisiblePaths();
    if (selected.isEmpty) return '';
    // The most recent selection is the keyboard paste target in both views.
    final anchor = _isFiltering
        ? _filterSelectionAnchor
        : _selectedRelativePath;
    final path = anchor != null && selected.contains(anchor)
        ? anchor
        : selected.first;
    final entry = _visibleEntryForPath(path);
    return _isDirectoryEntry(entry) ? path : _parentPath(path);
  }

  Future<void> _handleMenuAction(
    BuildContext context,
    tree.VisibleNode node,
    _ExplorerAction action,
  ) => _handleEntryMenuAction(
    context,
    entry: _entryByNodeId[node.id],
    action: action,
    nodeId: node.id,
  );

  Future<void> _handleEntryMenuAction(
    BuildContext context, {
    required native.WorkspaceFileEntry? entry,
    required _ExplorerAction action,
    String? nodeId,
  }) async {
    final targetDir = _isDirectoryEntry(entry)
        ? entry!.relativePath
        : _parentPath(entry?.relativePath ?? '');
    switch (action) {
      case _ExplorerAction.collapse:
        if (nodeId != null) {
          _controller.expansions.setExpanded(nodeId, false);
        }
        final relativePath = entry?.relativePath;
        if (relativePath != null) {
          _expandedDirectoryPaths.remove(relativePath);
        }
      case _ExplorerAction.newFile:
        await _createEntry(parentPath: targetDir, directory: false);
      case _ExplorerAction.newFolder:
        await _createEntry(parentPath: targetDir, directory: true);
      case _ExplorerAction.rename:
        if (entry != null) {
          await _rename(entry);
        }
      case _ExplorerAction.copy:
        if (entry != null) {
          await _copyToClipboard(entry, cut: false);
        }
      case _ExplorerAction.cut:
        if (entry != null) {
          await _copyToClipboard(entry, cut: true);
        }
      case _ExplorerAction.paste:
        await _paste(targetDir);
      case _ExplorerAction.copyPath:
        if (entry != null) {
          await Clipboard.setData(
            ClipboardData(
              text: userVisibleWorkspacePath(_absolutePath(entry.relativePath)),
            ),
          );
          _showInfo('Path copied');
        }
      case _ExplorerAction.copyRelativePath:
        if (entry != null) {
          await Clipboard.setData(ClipboardData(text: entry.relativePath));
          _showInfo('Relative path copied');
        }
      case _ExplorerAction.duplicate:
        if (entry != null) {
          await _duplicate(entry);
        }
      case _ExplorerAction.reveal:
        if (entry != null) {
          await _reveal(entry);
        }
      case _ExplorerAction.openWithDefaultApplication:
        if (entry != null && !_isDirectoryEntry(entry)) {
          await _openWithDefaultApplication(entry);
        }
      case _ExplorerAction.openExternally:
        final kind =
            _pickedExternalEditorKind ??
            (await ref.read(resolvedExternalEditorProvider.future))?.kind;
        _pickedExternalEditorKind = null;
        if (entry != null && kind != null) {
          await _openExternally(entry, kind);
        }
      case _ExplorerAction.openInAlera:
        if (entry != null && !_isDirectoryEntry(entry)) {
          widget.onOpenFileInAlera?.call(entry.relativePath);
        }
      case _ExplorerAction.viewFileHistory:
        if (entry != null && !_isDirectoryEntry(entry)) {
          await openWorkspaceFileHistory(
            context: context,
            ref: ref,
            workspace: widget.workspace,
            relativePath: entry.relativePath,
          );
        }
      case _ExplorerAction.compareLatestGitRevision:
        if (entry != null && !_isDirectoryEntry(entry)) {
          await compareWorkspaceFileWithLatestGitRevision(
            context: context,
            ref: ref,
            workspace: widget.workspace,
            relativePath: entry.relativePath,
            sourceControlScope: _gitCompareSourceControlScope,
          );
        }
      case _ExplorerAction.compareGitBranch:
        if (entry != null && !_isDirectoryEntry(entry)) {
          await compareWorkspaceFileWithBranch(
            context: context,
            ref: ref,
            workspace: widget.workspace,
            relativePath: entry.relativePath,
            sourceControlScope: _gitCompareSourceControlScope,
          );
        }
      case _ExplorerAction.delete:
        if (entry != null) {
          await _delete(entry);
        }
      case _ExplorerAction.refresh:
        await _refreshDirectory(targetDir);
      case _ExplorerAction.focusSourceControlRoot:
        if (_isDirectoryEntry(entry)) {
          await _focusSourceControlRoot(entry!);
        }
      case _ExplorerAction.clearSourceControlRoot:
        widget.onClearSourceControlRoot?.call();
    }
  }

  WorkspaceSourceControlScope get _gitCompareSourceControlScope {
    final relativeRoot = normalizeSourceControlRootRelativePath(
      widget.focusedSourceControlRoot,
    );
    if (relativeRoot == null) {
      return WorkspaceSourceControlScope(
        workspaceId: widget.workspace.id,
        workspacePath: widget.workspace.path,
        path: widget.workspace.path,
      );
    }
    return WorkspaceSourceControlScope(
      workspaceId: widget.workspace.id,
      workspacePath: widget.workspace.path,
      path: sourceControlRootAbsolutePath(
        workspacePath: widget.workspace.path,
        relativeRoot: relativeRoot,
      ),
      relativeRoot: relativeRoot,
    );
  }

  Future<void> _handleBackgroundAction(
    BuildContext context,
    _ExplorerAction action,
  ) async {
    switch (action) {
      case _ExplorerAction.newFile:
        await _createEntry(directory: false);
      case _ExplorerAction.newFolder:
        await _createEntry(directory: true);
      case _ExplorerAction.paste:
        await _paste('');
      case _:
        break;
    }
  }

  Future<void> _createEntry({
    String parentPath = '',
    required bool directory,
  }) async {
    final name = await _promptName(
      title: directory ? 'New folder' : 'New file',
      label: directory ? 'Folder name' : 'File name',
    );
    if (!mounted) {
      return;
    }
    if (name == null) {
      return;
    }
    try {
      if (directory) {
        await _workspaceFiles.createDirectory(
          workspacePath: widget.workspace.path,
          parentRelativePath: parentPath,
          name: name,
        );
      } else {
        await _workspaceFiles.createFile(
          workspacePath: widget.workspace.path,
          parentRelativePath: parentPath,
          name: name,
        );
      }
      if (!mounted) {
        return;
      }
      await _refreshDirectory(parentPath);
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _rename(native.WorkspaceFileEntry entry) async {
    final name = await _promptName(
      title: 'Rename',
      label: 'Name',
      initialValue: entry.name,
    );
    if (!mounted) {
      return;
    }
    if (name == null) {
      return;
    }
    try {
      final renamed = await _workspaceFiles.renameEntry(
        workspacePath: widget.workspace.path,
        relativePath: entry.relativePath,
        newName: name,
      );
      if (!mounted) {
        return;
      }
      await widget.onPathMoved(entry.relativePath, renamed.relativePath);
      if (!mounted) {
        return;
      }
      await _refreshDirectory(_parentPath(entry.relativePath));
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _copyToClipboard(
    native.WorkspaceFileEntry entry, {
    required bool cut,
  }) async {
    final relativePaths = _selectedPathsForEntry(entry);
    int? systemSequenceNumber;
    try {
      systemSequenceNumber = await _workspaceFiles.writeSystemFileClipboard(
        // Windows Explorer does not accept verbatim `\\?\` paths on the
        // system file clipboard.
        paths: <String>[
          for (final path in relativePaths)
            userVisibleWorkspacePath(_absolutePath(path)),
        ],
        operation: cut
            ? WorkspaceFileClipboardOperation.cut
            : WorkspaceFileClipboardOperation.copy,
      );
    } catch (error) {
      if (mounted) {
        _showError(error);
      }
    }
    if (!mounted) {
      return;
    }
    _setClipboard(
      _ExplorerClipboard(
        List<String>.unmodifiable(relativePaths),
        cut,
        systemSequenceNumber: systemSequenceNumber,
      ),
    );
  }

  Future<void> _paste(String targetDir) async {
    try {
      if (Platform.isWindows) {
        final systemClipboard = await _workspaceFiles.readSystemFileClipboard();
        if (systemClipboard != null) {
          if (_isInternalSystemClipboard(systemClipboard)) {
            await _pasteInternal(targetDir);
          } else {
            await _pasteExternal(systemClipboard, targetDir);
          }
          return;
        }

        final clipboard = _clipboard;
        if (clipboard == null) {
          return;
        }
        final expectedSequence = clipboard.systemSequenceNumber;
        if (expectedSequence != null) {
          final currentSequence = await _workspaceFiles
              .systemFileClipboardSequenceNumber();
          if (currentSequence != expectedSequence) {
            return;
          }
        }
      }
      await _pasteInternal(targetDir);
    } catch (error) {
      _showError(error);
    }
  }

  bool _isInternalSystemClipboard(WorkspaceFileClipboardPayload payload) {
    final clipboard = _clipboard;
    if (clipboard == null ||
        clipboard.systemSequenceNumber != payload.sequenceNumber ||
        payload.paths.length != clipboard.relativePaths.length ||
        clipboard.cut !=
            (payload.operation == WorkspaceFileClipboardOperation.cut)) {
      return false;
    }
    for (var index = 0; index < payload.paths.length; index++) {
      if (!p.equals(
        comparablePath(payload.paths[index]),
        comparablePath(_absolutePath(clipboard.relativePaths[index])),
      )) {
        return false;
      }
    }
    return true;
  }

  Future<void> _pasteInternal(String targetDir) async {
    final clipboard = _clipboard;
    if (clipboard == null) {
      return;
    }
    if (clipboard.cut) {
      final moved = await _moveSelectedEntries(
        clipboard.relativePaths,
        targetDir,
      );
      if (!moved || !mounted) {
        return;
      }
      final sequence = clipboard.systemSequenceNumber;
      if (sequence != null) {
        await _workspaceFiles.clearSystemFileClipboardIfSequence(sequence);
      }
      if (mounted) {
        _setClipboard(null);
      }
      return;
    }

    for (final path in clipboard.relativePaths) {
      await _workspaceFiles.copyEntry(
        workspacePath: widget.workspace.path,
        relativePath: path,
        targetParentRelativePath: targetDir,
      );
      if (!mounted) return;
    }
    await _refreshDirectory(targetDir);
  }

  Future<void> _pasteExternal(
    WorkspaceFileClipboardPayload clipboard,
    String targetDir,
  ) async {
    final moveSources =
        clipboard.operation == WorkspaceFileClipboardOperation.cut;
    final imported = await _workspaceFiles.importEntries(
      workspacePath: widget.workspace.path,
      sourcePaths: clipboard.paths,
      targetParentRelativePath: targetDir,
      moveSources: moveSources,
    );
    if (moveSources) {
      await _workspaceFiles.clearSystemFileClipboardIfSequence(
        clipboard.sequenceNumber,
      );
    }
    if (!mounted) {
      return;
    }

    if (moveSources) {
      for (var index = 0; index < clipboard.paths.length; index++) {
        // Paths cut in the OS file manager never carry the `\\?\` prefix a
        // stored workspace root may have.
        final relative = workspaceRelativePath(
          workspacePath: widget.workspace.path,
          filePath: clipboard.paths[index],
        );
        if (relative == null || relative == '.') {
          continue;
        }
        final oldRelativePath = relative.replaceAll('\\', '/');
        await widget.onPathMoved(oldRelativePath, imported[index].relativePath);
        if (!mounted) {
          return;
        }
      }
    }
    await _refreshDirectory(targetDir);
  }

  Future<bool> _moveSelectedEntries(
    List<String> relativePaths,
    String targetDir,
  ) async {
    if (relativePaths.any(
      (path) => path == targetDir || targetDir.startsWith('$path/'),
    )) {
      _showInfo('Cannot move a folder into itself.');
      return false;
    }
    for (final path in relativePaths) {
      if (!await _moveEntry(path, targetDir) || !mounted) {
        return false;
      }
    }
    return true;
  }

  Future<void> _duplicate(native.WorkspaceFileEntry entry) async {
    final parentPath = _parentPath(entry.relativePath);
    try {
      await _workspaceFiles.copyEntry(
        workspacePath: widget.workspace.path,
        relativePath: entry.relativePath,
        targetParentRelativePath: parentPath,
      );
      if (!mounted) {
        return;
      }
      await _refreshDirectory(parentPath);
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _revealPendingPath({String? relativePath}) async {
    final request = ref.read(workspaceExplorerRevealControllerProvider);
    final targetPath = normalizeWorkspaceRelativePath(
      relativePath ??
          (request?.workspaceId == widget.workspace.id
              ? request?.relativePath
              : null),
    );
    if (targetPath == null) {
      return;
    }
    final revealed = await _revealPathInTree(targetPath, clearFilter: true);
    if (!mounted || !revealed) {
      return;
    }
    if (request != null &&
        request.workspaceId == widget.workspace.id &&
        request.relativePath == targetPath) {
      ref
          .read(workspaceExplorerRevealControllerProvider.notifier)
          .consume(request);
    }
  }

  /// The tree's native reveal expands ancestors in its own controller; mirror
  /// those changes to the workspace snapshot as well. Otherwise switching
  /// workspaces restores the state before the search/reveal.
  Future<bool> _revealPathInTree(
    String relativePath, {
    bool clearFilter = false,
    bool expandDirectory = false,
  }) async {
    final generation = ++_treeRevealGeneration;
    final workspaceId = widget.workspace.id;
    final workspacePath = widget.workspace.path;
    bool stillCurrent() =>
        mounted &&
        generation == _treeRevealGeneration &&
        widget.workspace.id == workspaceId &&
        widget.workspace.path == workspacePath;

    if (clearFilter && _isFiltering) {
      _filterController.clear();
      _onFilterChanged('');
    }
    try {
      await _ensureAncestorsLoaded(relativePath);
    } catch (_) {
      // A stale search-index result can disappear before Explorer loads its
      // ancestors. Opening the requested file is still handled separately.
      return false;
    }
    if (!stillCurrent()) return false;
    final target = _entryByPath[relativePath];
    if (target == null) return false;

    if (expandDirectory &&
        _isDirectoryEntry(target) &&
        !_childrenByDirectory.containsKey(relativePath)) {
      await _loadDirectory(relativePath);
      if (!stillCurrent()) return false;
    }
    _rebuildTree();
    final targetNodeId = _nodeIdForRelativePath(relativePath);
    if (targetNodeId == null) return false;
    final ancestors = <String>[];
    var parent = _parentPath(relativePath);
    while (parent.isNotEmpty) {
      if (_isDirectoryEntry(_entryByPath[parent])) ancestors.add(parent);
      parent = _parentPath(parent);
    }
    for (final path in ancestors.reversed) {
      final nodeId = _nodeIdForRelativePath(path);
      if (nodeId == null) continue;
      _expandedDirectoryPaths.add(path);
      _controller.expansions.setExpanded(nodeId, true);
    }
    if (expandDirectory && _isDirectoryEntry(target)) {
      _expandedDirectoryPaths.add(relativePath);
      _controller.expansions.setExpanded(targetNodeId, true);
    }
    _selectedRelativePath = relativePath;
    _controller.selection.selectOnly(targetNodeId);
    // Save immediately, not only on widget disposal or the next project
    // switch. An in-flight scroll animation must not lose expansion state.
    _saveWorkspaceUiState(widget.workspace);
    // DirectoryTreeView is unmounted while filtered results are showing.
    // Retain its expansion/selection; scrolling resumes after filter clears.
    if (_isFiltering) return true;
    await WidgetsBinding.instance.endOfFrame;
    if (!stillCurrent()) return false;
    await _controller.reveal(nodeId: targetNodeId, select: true);
    if (!stillCurrent()) return false;
    _saveWorkspaceUiState(widget.workspace);
    return true;
  }

  Future<void> _ensureAncestorsLoaded(String relativePath) async {
    var ancestor = '';
    for (final part in relativePath.split('/')) {
      if (part.isEmpty) {
        continue;
      }
      if (!_childrenByDirectory.containsKey(ancestor)) {
        await _loadDirectory(ancestor);
        if (!mounted) {
          return;
        }
      }
      ancestor = ancestor.isEmpty ? part : '$ancestor/$part';
    }
  }

  String? _nodeIdForRelativePath(String relativePath) {
    for (final binding in _entryByNodeId.entries) {
      if (binding.value.relativePath == relativePath) {
        return binding.key;
      }
    }
    return 'path:$relativePath';
  }

  Future<void> _reveal(native.WorkspaceFileEntry entry) async {
    final result = await _folderOpener.reveal(
      _absolutePath(entry.relativePath),
    );
    if (!mounted) {
      return;
    }
    if (!result.ok) {
      _showError(result.message ?? 'Could not reveal item');
    }
  }

  Future<void> _openWithDefaultApplication(
    native.WorkspaceFileEntry entry,
  ) async {
    final result = await _folderOpener.openWithDefaultApplication(
      _absolutePath(entry.relativePath),
    );
    if (!mounted) {
      return;
    }
    if (!result.ok) {
      _showError(
        result.message ?? 'Could not open item with the default application',
      );
    }
  }

  Future<void> _openExternally(
    native.WorkspaceFileEntry entry,
    ExternalEditorKind kind,
  ) async {
    final launcher = ref.read(externalEditorLauncherForProvider(kind));
    final absolutePath = _absolutePath(entry.relativePath);
    final result = _isDirectoryEntry(entry)
        ? await launcher.openWorkspace(absolutePath)
        : await launcher.openFile(
            ExternalEditorOpenRequest(
              workspacePath: widget.workspace.path,
              filePath: absolutePath,
            ),
          );
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message:
            result.message ??
            'Could not open item in ${externalEditorSpecs[kind]?.displayName ?? 'the external editor'}.',
        tone: .error,
      );
    }
  }

  Future<bool> _moveEntry(String relativePath, String targetDir) async {
    try {
      final sourceParent = _parentPath(relativePath);
      final moved = await _workspaceFiles.moveEntry(
        workspacePath: widget.workspace.path,
        relativePath: relativePath,
        targetParentRelativePath: targetDir,
      );
      if (!mounted) {
        return false;
      }
      await widget.onPathMoved(relativePath, moved.relativePath);
      if (!mounted) {
        return false;
      }
      await _refreshDirectory(sourceParent);
      if (!mounted) {
        return false;
      }
      if (sourceParent != targetDir) {
        await _refreshDirectory(targetDir);
      }
      return true;
    } catch (error) {
      _showError(error);
      return false;
    }
  }

  Future<void> _delete(native.WorkspaceFileEntry entry) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AleraConfirmDialog(
        title: 'Delete ${entry.name}',
        message: 'Move this item to the trash?',
        confirmLabel: 'Delete',
        destructive: true,
      ),
    );
    if (confirmed != true) {
      return;
    }
    if (!mounted) {
      return;
    }
    try {
      await _workspaceFiles.deleteEntry(
        workspacePath: widget.workspace.path,
        relativePath: entry.relativePath,
      );
      if (!mounted) {
        return;
      }
      await _refreshDirectory(_parentPath(entry.relativePath));
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _saveAllEditors() async {
    try {
      final count = await _editorSessions.saveAll(_workspaceFiles);
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: count == 0
            ? 'No unsaved editor files'
            : count == 1
            ? 'Saved 1 editor file'
            : 'Saved $count editor files',
      );
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _refreshDirectory(
    String relativePath, {
    bool refreshGitStatus = true,
  }) async {
    try {
      if (refreshGitStatus) {
        await _refreshGitStatusSnapshot();
      }
      final previousStructureRevision = _treeStructureRevision;
      await _loadDirectory(relativePath);
      if (!mounted) return;
      // Metadata-only and identical watcher events retain TreeData and the
      // flattened visible list. Changed rows repaint through their notifier.
      // Only native graph/entry-binding changes require controller.rebuild.
      if (_treeStructureRevision != previousStructureRevision) {
        _rebuildTree();
      }
      // Keep the file filter index in sync with edits made by either view.
      if (_isFiltering) {
        _workspaceFiles.scheduleQuickOpenRefresh(
          workspacePath: widget.workspace.path,
          debounce: Duration.zero,
        );
      }
    } catch (error) {
      _showError(error);
    }
  }

  Future<void> _focusSourceControlRoot(native.WorkspaceFileEntry entry) async {
    final focus = widget.onFocusSourceControlFolder;
    if (focus == null) {
      return;
    }
    try {
      final focused = await focus(entry.relativePath);
      if (!mounted) {
        return;
      }
      if (!focused) {
        _showInfo('Folder is not a Git repository');
      }
    } catch (_) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not use folder as source control root',
          tone: .error,
        );
      }
    }
  }

  bool _canDrop(_ExplorerDragData data, String targetPath) {
    final target = _visibleEntryForPath(targetPath);
    if (!_isDirectoryEntry(target)) {
      return false;
    }
    return data.relativePaths.every(
      (path) => path != targetPath && !targetPath.startsWith('$path/'),
    );
  }

  void _toggleMode() {
    if (!mounted) {
      return;
    }
    widget.onModeChanged(
      widget.mode == WorkspaceExplorerMode.hideIgnored
          ? WorkspaceExplorerMode.showAll
          : WorkspaceExplorerMode.hideIgnored,
    );
  }

  void _toggleHiddenFiles() {
    if (!mounted) {
      return;
    }
    widget.onShowHiddenFilesChanged(!widget.showHiddenFiles);
  }

  void _showInfo(String message) {
    if (!mounted) {
      return;
    }
    AleraToast.show(context, message: message);
  }

  Future<String?> _promptName({
    required String title,
    required String label,
    String initialValue = '',
  }) async {
    final result = await showDialog<String>(
      context: context,
      builder: (context) => _ExplorerNameDialog(
        title: title,
        label: label,
        initialValue: initialValue,
      ),
    );
    final trimmed = result?.trim();
    return trimmed == null || trimmed.isEmpty ? null : trimmed;
  }

  void _showError(Object error) {
    if (!mounted) {
      return;
    }
    AleraToast.show(
      context,
      message: context.tr(_errorMessage(error)),
      tone: .error,
    );
  }

  String _errorMessage(Object error) {
    if (error is native.WorkspaceFileError) {
      return switch (error.kind) {
        native.WorkspaceFileErrorKind.alreadyExists => 'Item already exists',
        native.WorkspaceFileErrorKind.protectedPath => 'Path is protected',
        native.WorkspaceFileErrorKind.outsideWorkspace =>
          'Path is outside the workspace',
        native.WorkspaceFileErrorKind.unsupported => 'Operation is unsupported',
        native.WorkspaceFileErrorKind.notFound => 'Item not found',
        native.WorkspaceFileErrorKind.conflict => 'File changed on disk',
        _ => 'File operation failed',
      };
    }
    return 'File operation failed';
  }

  String _parentPath(String relativePath) {
    final index = relativePath.lastIndexOf('/');
    return index < 0 ? '' : relativePath.substring(0, index);
  }

  String _absolutePath(String relativePath) {
    return terminalAbsolutePath(
      rootPath: widget.workspace.path,
      relativePath: relativePath,
    );
  }
}
