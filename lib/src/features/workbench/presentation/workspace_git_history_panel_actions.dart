part of 'workspace_git_diff_panel.dart';

extension _GitHistoryPanelActions on _GitHistoryPanelState {
  Future<void> _openRefActions(
    GitHistoryItemRef itemRef,
    Offset globalPosition,
  ) async {
    final currentRef = widget.state.result?.currentRef;
    final isCurrentBranch = currentRef?.id == itemRef.id;
    final isCurrentUpstream = gitHistoryRefMatchesUpstream(
      itemRef,
      remoteRef: widget.state.result?.remoteRef,
      upstream: widget.currentUpstream,
    );
    final action = await showGitHistoryRefMenu(
      context,
      itemRef,
      globalPosition,
      isCurrentBranch: isCurrentBranch,
      canPullIntoCurrentBranch: isCurrentUpstream,
    );
    if (!mounted || action == null) {
      return;
    }
    await widget.onRefAction(
      itemRef,
      action,
      isCurrentBranch: isCurrentBranch,
      isCurrentUpstream: isCurrentUpstream,
      currentBranch: currentRef?.name,
    );
  }

  Future<void> _openBoundaryActions(Offset globalPosition) async {
    final action = await showGitHistoryBoundaryMenu(context, globalPosition);
    if (!mounted || action == null) {
      return;
    }
    await widget.onBoundaryAction(action);
  }
}
