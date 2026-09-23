part of 'workspace_git_diff_panel.dart';

enum _GitChangeContextAction {
  openFile,
  openExternally,
  revealInExplorer,
  addToGitIgnore,
  stage,
  unstage,
  discard,
}

Future<void> _showGitChangeContextMenu(
  BuildContext context,
  Offset position, {
  required bool canOpenFile,
  required bool canStage,
  required bool canUnstage,
  required bool canDiscard,
  required bool busy,
  required VoidCallback? onOpenFile,
  required ExternalEditorSpec? externalEditor,
  required List<ExternalEditorSpec> installedExternalEditors,
  required ValueChanged<ExternalEditorKind>? onOpenExternally,
  required VoidCallback onRevealInExplorer,
  required VoidCallback? onAddToGitIgnore,
  required VoidCallback onStage,
  required VoidCallback onUnstage,
  required VoidCallback onDiscard,
}) async {
  ExternalEditorKind? pickedKind;
  final selected = await showMenu<_GitChangeContextAction>(
    context: context,
    position: .fromLTRB(position.dx, position.dy, position.dx, position.dy),
    items: <PopupMenuEntry<_GitChangeContextAction>>[
      if (canOpenFile)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .openFile,
          label: 'Open File',
          leading: Icon(AleraIcons.file, size: 16),
        ),
      if (externalEditor case final editor?)
        AleraDropdownSubmenuEntry<_GitChangeContextAction, ExternalEditorKind>(
          primaryValue: .openExternally,
          label: 'Open in ${editor.shortName}',
          leading: const Icon(AleraIcons.external, size: 16),
          childResult: (_) => .openExternally,
          onChildResult: (kind) => pickedKind = kind,
          children: externalEditorMenuChildren(
            resolved: editor,
            installed: installedExternalEditors,
          ),
        ),
      const AleraDropdownEntry<_GitChangeContextAction>(
        value: .revealInExplorer,
        label: 'Reveal in Explorer',
        leading: Icon(AleraIcons.copyFiles, size: 16),
      ),
      if (onAddToGitIgnore != null)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .addToGitIgnore,
          label: 'Add to .gitignore',
          leading: Icon(AleraIcons.file, size: 16),
        ),
      if (canStage || canUnstage || canDiscard)
        const PopupMenuDivider(height: AleraTokens.space8),
      if (canUnstage)
        AleraDropdownEntry<_GitChangeContextAction>(
          value: .unstage,
          label: 'Unstage',
          leading: const Icon(AleraIcons.gitUnstage, size: 16),
          enabled: !busy,
        ),
      if (canStage)
        AleraDropdownEntry<_GitChangeContextAction>(
          value: .stage,
          label: 'Stage',
          leading: const Icon(AleraIcons.gitStage, size: 16),
          enabled: !busy,
        ),
      if (canDiscard)
        AleraDropdownEntry<_GitChangeContextAction>(
          value: .discard,
          label: 'Discard',
          leading: const Icon(AleraIcons.gitDiscard, size: 16),
          enabled: !busy,
        ),
    ],
  );
  if (selected == null || !context.mounted) {
    return;
  }
  switch (selected) {
    case _GitChangeContextAction.openFile:
      onOpenFile?.call();
    case _GitChangeContextAction.openExternally:
      final kind = pickedKind ?? externalEditor?.kind;
      if (kind != null) {
        onOpenExternally?.call(kind);
      }
    case _GitChangeContextAction.revealInExplorer:
      onRevealInExplorer();
    case _GitChangeContextAction.addToGitIgnore:
      onAddToGitIgnore?.call();
    case _GitChangeContextAction.stage:
      onStage();
    case _GitChangeContextAction.unstage:
      onUnstage();
    case _GitChangeContextAction.discard:
      onDiscard();
  }
}

extension _WorkspaceGitDiffPanelGitIgnore on _WorkspaceGitDiffPanelState {
  Future<void> _addToGitIgnore(String path, {required bool isDirectory}) async {
    try {
      final pattern = _gitIgnorePattern(path, isDirectory: isDirectory);
      final gitIgnore = File(
        '${widget.sourceControlScope.path}${Platform.pathSeparator}.gitignore',
      );
      final added = await _appendGitIgnorePattern(gitIgnore, pattern);
      if (added) {
        try {
          await _notifier.refresh();
        } on Object {
          // The source-control watcher will refresh again after the file write.
        }
      }
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: added ? 'Added to .gitignore' : 'Already in .gitignore',
        tone: .success,
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    }
  }
}

String _gitIgnorePattern(String path, {required bool isDirectory}) {
  final normalized = path
      .replaceAll('\\', '/')
      .split('/')
      .where((segment) => segment.isNotEmpty)
      .join('/');
  final escaped = normalized.replaceAllMapped(
    RegExp(r'[\\*?\[\]#! ]'),
    (match) => '\\${match.group(0)}',
  );
  return '/$escaped${isDirectory ? '/' : ''}';
}

Future<bool> _appendGitIgnorePattern(File file, String pattern) async {
  if (!await file.exists()) {
    await file.writeAsString('$pattern\n', flush: true);
    return true;
  }

  final content = await file.readAsString();
  final alreadyPresent = content
      .split(RegExp(r'\r?\n'))
      .any((line) => line == pattern);
  if (alreadyPresent) {
    return false;
  }

  final newline = content.contains('\r\n') ? '\r\n' : '\n';
  final separator =
      content.isEmpty || content.endsWith('\n') || content.endsWith('\r')
      ? ''
      : newline;
  await file.writeAsString(
    '$separator$pattern$newline',
    mode: FileMode.append,
    flush: true,
  );
  return true;
}
