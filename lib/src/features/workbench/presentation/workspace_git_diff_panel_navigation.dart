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

  List<String> _openableChangedSourcePaths(WorkspaceSourceControlState? state) {
    if (state == null) return const <String>[];
    final seen = <String>{};
    return <String>[
      for (final entry in state.status.entries)
        if (entry.status != GitChangeStatus.deleted &&
            (entry.submodule == null || entry.isSubmoduleChild) &&
            seen.add(entry.path))
          entry.path,
    ];
  }

  Future<void> _openChangesInZed(WorkspaceSourceControlState? state) async {
    final absolutePaths = <String>[];
    for (final sourceRelativePath in _openableChangedSourcePaths(state)) {
      final workspaceRelativePath = widget.sourceControlScope
          .toWorkspaceRelativePath(sourceRelativePath);
      if (workspaceRelativePath == null) continue;
      absolutePaths.add(
        terminalAbsolutePath(
          rootPath: widget.workspace.path,
          relativePath: workspaceRelativePath,
        ),
      );
    }
    if (absolutePaths.isEmpty) return;
    final result = await ref
        .read(externalEditorLauncherProvider)
        .openFiles(
          ExternalEditorOpenFilesRequest(
            workspacePath: widget.workspace.path,
            filePaths: absolutePaths,
          ),
        );
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message: result.message ?? 'Could not open changed files in Zed.',
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
