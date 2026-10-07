part of 'workspace_git_diff_panel.dart';

enum _GitChangeContextAction {
  openFile,
  openWithDefaultApplication,
  openExternally,
  revealInExplorer,
  viewFileHistory,
  compareLatestGitRevision,
  compareGitBranch,
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
  required VoidCallback? onOpenWithDefaultApplication,
  required ExternalEditorSpec? externalEditor,
  required List<ExternalEditorSpec> installedExternalEditors,
  required ValueChanged<ExternalEditorKind>? onOpenExternally,
  required VoidCallback onRevealInExplorer,
  required VoidCallback? onViewFileHistory,
  required VoidCallback? onCompareLatestGitRevision,
  required VoidCallback? onCompareGitBranch,
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
      if (onOpenWithDefaultApplication != null)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .openWithDefaultApplication,
          label: 'Open with Default Application',
          leading: Icon(AleraIcons.external, size: 16),
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
      if (onViewFileHistory != null ||
          onCompareLatestGitRevision != null ||
          onCompareGitBranch != null)
        const PopupMenuDivider(height: AleraTokens.space8),
      if (onViewFileHistory != null)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .viewFileHistory,
          label: 'View File History',
          leading: Icon(AleraIcons.gitGraph, size: 16),
        ),
      if (onCompareLatestGitRevision != null)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .compareLatestGitRevision,
          label: 'Compare With Latest Git Revision',
          leading: Icon(AleraIcons.diff, size: 16),
        ),
      if (onCompareGitBranch != null)
        const AleraDropdownEntry<_GitChangeContextAction>(
          value: .compareGitBranch,
          label: 'Compare With Branch...',
          leading: Icon(AleraIcons.gitBranch, size: 16),
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
    case _GitChangeContextAction.openWithDefaultApplication:
      onOpenWithDefaultApplication?.call();
    case _GitChangeContextAction.openExternally:
      final kind = pickedKind ?? externalEditor?.kind;
      if (kind != null) {
        onOpenExternally?.call(kind);
      }
    case _GitChangeContextAction.revealInExplorer:
      onRevealInExplorer();
    case _GitChangeContextAction.viewFileHistory:
      onViewFileHistory?.call();
    case _GitChangeContextAction.compareLatestGitRevision:
      onCompareLatestGitRevision?.call();
    case _GitChangeContextAction.compareGitBranch:
      onCompareGitBranch?.call();
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
      final added = await addWorkspacePathToGitIgnore(
        rootPath: widget.sourceControlScope.path,
        path: path,
        isDirectory: isDirectory,
      );
      if (added) {
        // Do not block the context-menu action on a full source-control reload.
        // The watcher will refresh after the file write, while this best-effort
        // refresh keeps the UI responsive on platforms where watcher delivery
        // is delayed or unavailable.
        unawaited(_notifier.refresh().catchError((_) {}));
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
