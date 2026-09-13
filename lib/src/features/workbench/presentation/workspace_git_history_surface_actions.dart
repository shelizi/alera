part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistorySurfaceActions
    on _WorkspaceGitHistorySurfaceState {
  Future<void> _openCommitMenu(
    GitHistoryItem item,
    Offset globalPosition,
  ) async {
    final action = await showGitHistoryCommitMenu(
      context,
      item,
      globalPosition,
    );
    if (!mounted || action == null) {
      return;
    }
    switch (action) {
      case GitHistoryCommitMenuAction.copyHash:
        await _copyCommitText(item.id, 'Commit Hash');
      case GitHistoryCommitMenuAction.copySubject:
        await _copyCommitText(
          item.message.trim().isEmpty ? item.subject : item.message,
          'Commit Subject',
        );
    }
  }

  Future<void> _openRefMenu(
    GitHistoryItemRef itemRef,
    Offset globalPosition,
  ) async {
    final action = await showGitHistoryRefMenu(
      context,
      itemRef,
      globalPosition,
      isCurrentBranch: _currentRef?.id == itemRef.id,
    );
    if (!mounted || action == null) {
      return;
    }
    switch (action) {
      case GitHistoryRefMenuAction.switchBranch:
        await _switchToRef(itemRef);
      case GitHistoryRefMenuAction.copyName:
        await _copyCommitText(itemRef.name, 'Branch Name');
    }
  }

  Future<void> _switchToRef(GitHistoryItemRef itemRef) async {
    final project = ref
        .read(workbenchControllerProvider)
        .projects
        .where((candidate) => candidate.id == widget.workspace.projectId)
        .firstOrNull;
    if (project == null) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not switch to ${itemRef.name}: project not found',
          tone: .error,
        );
      }
      return;
    }
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .switchWorkspaceBranch(
            project: project,
            workspace: widget.workspace,
            branch: itemRef.name,
          );
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: 'Switched to ${itemRef.name}',
        tone: .success,
      );
      _compareCache.clear();
      await _reload();
      if (!mounted) {
        return;
      }
      await ref
          .read(
            workspaceSourceControlControllerProvider(_sourceControlScope.path)
                .notifier,
          )
          .refresh();
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: error.toString(), tone: .error);
      }
    }
  }

  Future<void> _copyCommitText(String text, String label) async {
    try {
      await Clipboard.setData(ClipboardData(text: text));
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: '$label copied', tone: .success);
    } catch (_) {
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: 'Could not copy $label', tone: .error);
    }
  }
}
