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

  Future<void> _openWorkspaceFileWithDefaultApplication(
    String sourceRelativePath,
  ) async {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    final result = await ref
        .read(workspaceFolderOpenerProvider)
        .openWithDefaultApplication(
          terminalAbsolutePath(
            rootPath: widget.workspace.path,
            relativePath: workspaceRelativePath,
          ),
        );
    if (!mounted || result.ok) {
      return;
    }
    AleraToast.show(
      context,
      message:
          result.message ?? 'Could not open item with the default application.',
      tone: .error,
    );
  }

  Future<void> _openWorkspaceFileExternally(
    String sourceRelativePath,
    ExternalEditorKind kind,
  ) async {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    final spec = externalEditorSpecs[kind];
    final result = await ref
        .read(externalEditorLauncherForProvider(kind))
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
        message:
            result.message ??
            'Could not open file in ${spec?.displayName ?? 'the external editor'}.',
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

  Future<void> _openChangesExternally(
    WorkspaceSourceControlState? state,
    ExternalEditorKind kind,
  ) async {
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
    final spec = externalEditorSpecs[kind];
    final result = await ref
        .read(externalEditorLauncherForProvider(kind))
        .openFiles(
          ExternalEditorOpenFilesRequest(
            workspacePath: widget.workspace.path,
            filePaths: absolutePaths,
          ),
        );
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message:
            result.message ??
            'Could not open changed files in ${spec?.displayName ?? 'the external editor'}.',
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
    unawaited(_revealWorkspaceItemInFileManager(workspaceRelativePath));
  }

  void _openSourceFileHistory(String sourceRelativePath) {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    unawaited(
      openWorkspaceFileHistory(
        context: context,
        ref: ref,
        workspace: widget.workspace,
        relativePath: workspaceRelativePath,
      ),
    );
  }

  void _compareSourceFileWithLatestGitRevision(String sourceRelativePath) {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    unawaited(
      compareWorkspaceFileWithLatestGitRevision(
        context: context,
        ref: ref,
        workspace: widget.workspace,
        relativePath: workspaceRelativePath,
        sourceControlScope: widget.sourceControlScope,
      ),
    );
  }

  void _compareSourceFileWithBranch(String sourceRelativePath) {
    final workspaceRelativePath = widget.sourceControlScope
        .toWorkspaceRelativePath(sourceRelativePath);
    if (workspaceRelativePath == null) {
      return;
    }
    unawaited(
      compareWorkspaceFileWithBranch(
        context: context,
        ref: ref,
        workspace: widget.workspace,
        relativePath: workspaceRelativePath,
        sourceControlScope: widget.sourceControlScope,
      ),
    );
  }

  Future<void> _revealWorkspaceItemInFileManager(String relativePath) async {
    final result = await ref
        .read(workspaceFolderOpenerProvider)
        .reveal(
          terminalAbsolutePath(
            rootPath: widget.workspace.path,
            relativePath: relativePath,
          ),
        );
    if (!mounted || result.ok) {
      return;
    }
    AleraToast.show(
      context,
      message: result.message ?? 'Could not reveal item in file manager.',
      tone: .error,
    );
  }
}
