part of 'workspace_git_diff_panel.dart';

extension _WorkspaceGitDiffPanelNavigation on _WorkspaceGitDiffPanelState {
  void _openWorkspaceFile(String sourceRelativePath) {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    widget.onOpenFile?.call(workspaceRelativePath);
  }

  Future<void> _openWorkspaceFileInZed(String sourceRelativePath) async {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    final result = await ref
        .read(externalEditorLauncherProvider)
        .openFile(
          ExternalEditorOpenRequest(
            workspacePath: widget.workspace.path,
            filePath: terminalAbsolutePath(
              rootPath: widget.workspace.path,
              relativePath: workspaceRelativePath,
            ),
          ),
        );
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message: result.message ?? 'Could not open file in Zed.',
        tone: .error,
      );
    }
  }

  void _revealInExplorer(String sourceRelativePath) {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    final onRevealInExplorer = widget.onRevealInExplorer;
    if (onRevealInExplorer != null) {
      onRevealInExplorer(workspaceRelativePath);
      return;
    }
    ref
        .read(workspaceExplorerRevealControllerProvider.notifier)
        .reveal(
          workspaceId: widget.workspace.id,
          relativePath: workspaceRelativePath,
        );
  }
}
