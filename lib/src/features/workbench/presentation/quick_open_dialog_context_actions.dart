part of 'quick_open_dialog.dart';

/// Quick Open shares the Explorer's menu *presentation*, while these actions
/// resolve the active workspace and the indexed (potentially unloaded) path.
extension _QuickOpenContextActions on _QuickOpenDialogState {
  void _handleQuickOpenIndexRefreshed(String workspacePath) {
    if (!mounted) return;
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null || workspace.path != workspacePath) return;
    _updateQuery(_queryController.text);
  }

  void _selectQuickOpenContextTarget(int index) {
    if (index < 0 || index >= _matches.length) return;
    final path = _matches[index].relativePath;
    setState(() {
      _selectedIndex = index;
      if (!_contextSelectedPaths.contains(path)) {
        _contextSelectedPaths
          ..clear()
          ..add(path);
      }
      _contextSelectionAnchor = path;
    });
  }

  bool _handleQuickOpenModifiedSelection(int index) {
    final keyboard = HardwareKeyboard.instance;
    final path = _matches[index].relativePath;
    if (keyboard.isShiftPressed && _contextSelectionAnchor != null) {
      final anchor = _matches.indexWhere(
        (match) => match.relativePath == _contextSelectionAnchor,
      );
      if (anchor >= 0) {
        final first = anchor < index ? anchor : index;
        final last = anchor > index ? anchor : index;
        setState(() {
          _selectedIndex = index;
          _contextSelectedPaths
            ..clear()
            ..addAll(
              _matches
                  .skip(first)
                  .take(last - first + 1)
                  .map((match) => match.relativePath),
            );
        });
        return true;
      }
    }
    if (keyboard.isControlPressed || keyboard.isMetaPressed) {
      setState(() {
        _selectedIndex = index;
        if (!_contextSelectedPaths.add(path)) {
          _contextSelectedPaths.remove(path);
        }
        _contextSelectionAnchor = path;
      });
      return true;
    }
    return false;
  }

  List<String> _contextPathsFor(String path) {
    if (!_contextSelectedPaths.contains(path)) return <String>[path];
    return <String>[
      for (final match in _matches)
        if (_contextSelectedPaths.contains(match.relativePath))
          match.relativePath,
    ];
  }

  String _quickOpenAbsolutePath(Workspace workspace, String relativePath) =>
      terminalAbsolutePath(
        rootPath: workspace.path,
        relativePath: relativePath,
      );

  String _quickOpenParent(String path) =>
      path.contains('/') ? path.substring(0, path.lastIndexOf('/')) : '';

  void _quickOpenNotice(String message, {bool isError = false}) {
    if (!mounted) return;
    AleraToast.show(
      context,
      message: message,
      tone: isError ? AleraToastTone.error : AleraToastTone.info,
    );
  }

  void _refreshQuickOpenIndex(Workspace workspace) {
    _workspaceFiles.scheduleQuickOpenRefresh(
      workspacePath: workspace.path,
      debounce: Duration.zero,
    );
  }

  Future<void> _handleQuickOpenContextAction(
    BuildContext menuContext,
    String relativePath,
    String action,
  ) async {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null) return;
    final parentPath = _quickOpenParent(relativePath);
    try {
      switch (action) {
        case 'copy':
        case 'cut':
          await _copyQuickOpenFiles(
            workspace,
            relativePath,
            cut: action == 'cut',
          );
        case 'paste':
          await _pasteQuickOpenFiles(workspace, parentPath);
        case 'copyPath':
          await Clipboard.setData(
            ClipboardData(
              text: userVisibleWorkspacePath(
                _quickOpenAbsolutePath(workspace, relativePath),
              ),
            ),
          );
          _quickOpenNotice('Path copied');
        case 'copyRelativePath':
          await Clipboard.setData(ClipboardData(text: relativePath));
          _quickOpenNotice('Relative path copied');
        case 'duplicate':
          await _workspaceFiles.copyEntry(
            workspacePath: workspace.path,
            relativePath: relativePath,
            targetParentRelativePath: parentPath,
          );
          _refreshQuickOpenIndex(workspace);
        case 'reveal':
          final result = await ref
              .read(workspaceFolderOpenerProvider)
              .reveal(_quickOpenAbsolutePath(workspace, relativePath));
          if (!result.ok) {
            _quickOpenNotice(
              result.message ?? 'Could not reveal item',
              isError: true,
            );
          }
        case 'openWithDefaultApplication':
          final result = await ref
              .read(workspaceFolderOpenerProvider)
              .openWithDefaultApplication(
                _quickOpenAbsolutePath(workspace, relativePath),
              );
          if (!result.ok) {
            _quickOpenNotice(
              result.message ?? 'Could not open item',
              isError: true,
            );
          }
        case 'openExternally':
          final kind =
              _pickedExternalEditorKind ??
              (await ref.read(resolvedExternalEditorProvider.future))?.kind;
          _pickedExternalEditorKind = null;
          if (kind != null) {
            final result = await ref
                .read(externalEditorLauncherForProvider(kind))
                .openFile(
                  ExternalEditorOpenRequest(
                    workspacePath: workspace.path,
                    filePath: _quickOpenAbsolutePath(workspace, relativePath),
                  ),
                );
            if (!result.ok) {
              _quickOpenNotice(
                'Could not open item in external editor',
                isError: true,
              );
            }
          }
        case 'openInAlera':
          if (mounted) _openFile(relativePath, forceAlera: true);
        case 'viewFileHistory':
          await openWorkspaceFileHistory(
            context: menuContext,
            ref: ref,
            workspace: workspace,
            relativePath: relativePath,
          );
        case 'compareLatestGitRevision':
          await compareWorkspaceFileWithLatestGitRevision(
            context: menuContext,
            ref: ref,
            workspace: workspace,
            relativePath: relativePath,
          );
        case 'compareGitBranch':
          await compareWorkspaceFileWithBranch(
            context: menuContext,
            ref: ref,
            workspace: workspace,
            relativePath: relativePath,
          );
        case 'rename':
          final name = await _promptQuickOpenName(
            title: 'Rename',
            initialValue: p.basename(relativePath),
          );
          if (name == null ||
              !mounted ||
              ref.read(workbenchControllerProvider).activeWorkspaceId !=
                  workspace.id) {
            return;
          }
          final moved = await _workspaceFiles.renameEntry(
            workspacePath: workspace.path,
            relativePath: relativePath,
            newName: name,
          );
          try {
            await _syncQuickOpenPathMove(
              workspace,
              relativePath,
              moved.relativePath,
            );
          } finally {
            // A completed filesystem rename still needs a fresh search index
            // even if persisting an open tab's new path fails.
            if (mounted) _refreshQuickOpenIndex(workspace);
          }
        case 'newFile':
        case 'newFolder':
          final isFolder = action == 'newFolder';
          final name = await _promptQuickOpenName(
            title: isFolder ? 'New folder' : 'New file',
          );
          if (name == null ||
              !mounted ||
              ref.read(workbenchControllerProvider).activeWorkspaceId !=
                  workspace.id) {
            return;
          }
          if (isFolder) {
            await _workspaceFiles.createDirectory(
              workspacePath: workspace.path,
              parentRelativePath: parentPath,
              name: name,
            );
          } else {
            await _workspaceFiles.createFile(
              workspacePath: workspace.path,
              parentRelativePath: parentPath,
              name: name,
            );
          }
          _refreshQuickOpenIndex(workspace);
        case 'delete':
          final confirmed = await showDialog<bool>(
            context: menuContext,
            builder: (context) => AleraConfirmDialog(
              title: 'Delete ${p.basename(relativePath)}',
              message: 'Move this item to the trash?',
              confirmLabel: 'Delete',
              destructive: true,
            ),
          );
          if (confirmed != true ||
              !mounted ||
              ref.read(workbenchControllerProvider).activeWorkspaceId !=
                  workspace.id) {
            return;
          }
          await _workspaceFiles.deleteEntry(
            workspacePath: workspace.path,
            relativePath: relativePath,
          );
          _refreshQuickOpenIndex(workspace);
        case 'refresh':
          _refreshQuickOpenIndex(workspace);
        case 'collapse':
        case 'focusSourceControlRoot':
        case 'clearSourceControlRoot':
          // These tree/folder-specific items do not apply to Quick Open files.
          return;
      }
    } catch (error) {
      _quickOpenNotice('File operation failed: $error', isError: true);
    }
  }

  Future<String?> _promptQuickOpenName({
    required String title,
    String initialValue = '',
  }) async {
    return showDialog<String>(
      context: context,
      builder: (context) =>
          _QuickOpenNameDialog(title: title, initialValue: initialValue),
    );
  }

  Future<void> _syncQuickOpenPathMove(
    Workspace workspace,
    String oldPath,
    String newPath,
  ) async {
    await ref
        .read(workbenchControllerProvider.notifier)
        .syncFileTabsAfterPathMove(
          workspace: workspace,
          oldRelativePath: oldPath,
          newRelativePath: newPath,
        );
    ref
        .read(editorSessionRegistryProvider)
        .updateDocumentPathsAfterMove(
          workspacePath: workspace.path,
          oldRelativePath: oldPath,
          newRelativePath: newPath,
        );
  }

  Future<void> _copyQuickOpenFiles(
    Workspace workspace,
    String relativePath, {
    required bool cut,
  }) async {
    final paths = _contextPathsFor(relativePath);
    int? sequence;
    try {
      sequence = await _workspaceFiles.writeSystemFileClipboard(
        paths: <String>[
          for (final path in paths)
            userVisibleWorkspacePath(_quickOpenAbsolutePath(workspace, path)),
        ],
        operation: cut
            ? WorkspaceFileClipboardOperation.cut
            : WorkspaceFileClipboardOperation.copy,
      );
    } catch (error) {
      _quickOpenNotice(
        'Could not write system file clipboard: $error',
        isError: true,
      );
    }
    if (!mounted) return;
    _contextClipboard = _QuickOpenContextClipboard(
      paths: paths,
      cut: cut,
      systemSequenceNumber: sequence,
    );
  }

  Future<void> _pasteQuickOpenFiles(
    Workspace workspace,
    String targetParent,
  ) async {
    final systemClipboard = Platform.isWindows
        ? await _workspaceFiles.readSystemFileClipboard()
        : null;
    final own = _contextClipboard;
    if (systemClipboard != null &&
        (own == null ||
            own.systemSequenceNumber != systemClipboard.sequenceNumber ||
            own.cut !=
                (systemClipboard.operation ==
                    WorkspaceFileClipboardOperation.cut) ||
            own.paths.length != systemClipboard.paths.length ||
            !List.generate(own.paths.length, (i) => i).every(
              (i) => p.equals(
                userVisibleWorkspacePath(
                  _quickOpenAbsolutePath(workspace, own.paths[i]),
                ),
                systemClipboard.paths[i],
              ),
            ))) {
      final imported = await _workspaceFiles.importEntries(
        workspacePath: workspace.path,
        sourcePaths: systemClipboard.paths,
        targetParentRelativePath: targetParent,
        moveSources:
            systemClipboard.operation == WorkspaceFileClipboardOperation.cut,
      );
      if (systemClipboard.operation == WorkspaceFileClipboardOperation.cut) {
        for (var i = 0; i < imported.length; i++) {
          final oldPath = workspaceRelativePath(
            workspacePath: workspace.path,
            filePath: systemClipboard.paths[i],
          );
          if (oldPath != null && oldPath != '.') {
            await _syncQuickOpenPathMove(
              workspace,
              oldPath.replaceAll('\\', '/'),
              imported[i].relativePath,
            );
          }
        }
        await _workspaceFiles.clearSystemFileClipboardIfSequence(
          systemClipboard.sequenceNumber,
        );
      }
      if (mounted) _refreshQuickOpenIndex(workspace);
      return;
    }
    if (own == null) return;
    for (final path in own.paths) {
      if (own.cut) {
        if (path == targetParent || targetParent.startsWith('$path/')) {
          _quickOpenNotice('Cannot move a folder into itself.', isError: true);
          return;
        }
        final moved = await _workspaceFiles.moveEntry(
          workspacePath: workspace.path,
          relativePath: path,
          targetParentRelativePath: targetParent,
        );
        await _syncQuickOpenPathMove(workspace, path, moved.relativePath);
      } else {
        await _workspaceFiles.copyEntry(
          workspacePath: workspace.path,
          relativePath: path,
          targetParentRelativePath: targetParent,
        );
      }
      if (!mounted) return;
    }
    if (own.cut) {
      if (own.systemSequenceNumber case final sequence?) {
        await _workspaceFiles.clearSystemFileClipboardIfSequence(sequence);
      }
      _contextClipboard = null;
    }
    if (mounted) _refreshQuickOpenIndex(workspace);
  }
}

class const _QuickOpenContextClipboard({
  required final List<String> paths,
  required final bool cut,
  final int? systemSequenceNumber,
});

class const _QuickOpenNameDialog({
  required final String title,
  required final String initialValue,
}) extends StatefulWidget {
  @override
  State<_QuickOpenNameDialog> createState() => _QuickOpenNameDialogState();
}

class _QuickOpenNameDialogState extends State<_QuickOpenNameDialog> {
  late final TextEditingController _nameController;

  @override
  void initState() {
    super.initState();
    _nameController = TextEditingController(text: widget.initialValue);
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _nameController.text.trim();
    if (name.isNotEmpty) Navigator.of(context).pop(name);
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(context.tr(widget.title)),
    content: AleraTextField(
      controller: _nameController,
      autofocus: true,
      labelText: context.tr('Name'),
      onSubmitted: (_) => _submit(),
    ),
    actions: <Widget>[
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: Text(context.tr('Cancel')),
      ),
      FilledButton(onPressed: _submit, child: Text(context.tr('Create'))),
    ],
  );
}
