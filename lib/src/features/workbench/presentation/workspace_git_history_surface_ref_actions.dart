part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistorySurfaceRefActions
    on _WorkspaceGitHistorySurfaceState {
  Future<void> _openBoundaryMenu(Offset globalPosition) async {
    final action = await showGitHistoryBoundaryMenu(context, globalPosition);
    if (!mounted || action == null) {
      return;
    }
    await runGitHistoryBoundaryMenuAction(
      context: context,
      action: action,
      onStash: () => _withSourceControl((notifier) => notifier.stash()),
      onDiscardAll: _discardAllChanges,
      onCommitChanges: _openSourceControlPanel,
      onMutationSuccess: _refreshAfterHistoryMutation,
      errorMessage: (error) => error.toString(),
    );
  }

  Future<void> _discardAllChanges() async {
    await _withSourceControl((notifier) async {
      await notifier.discard(null);
      await notifier.discardArea(GitChangeArea.staged);
    });
  }

  Future<void> _refreshAfterHistoryMutation() async {
    if (!mounted) {
      return;
    }
    _compareCache.clear();
    await _reload();
    if (!mounted) {
      return;
    }
    await _withSourceControl((notifier) => notifier.refresh());
  }

  void _openSourceControlPanel() {
    final controller = ref.read(workbenchControllerProvider.notifier);
    controller.setRightSidebarVisible(true);
    controller.setContextPanelTab(WorkbenchContextPanelTab.gitDiff);
  }
}
