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
      case GitHistoryCommitMenuAction.checkoutCommit:
        await _checkoutCommit(item);
      case GitHistoryCommitMenuAction.revertCommit:
        await _revertCommit(item);
      case GitHistoryCommitMenuAction.resetSoft:
        await _resetToCommit(item, GitResetMode.soft);
      case GitHistoryCommitMenuAction.resetMixed:
        await _resetToCommit(item, GitResetMode.mixed);
      case GitHistoryCommitMenuAction.resetHard:
        await _resetToCommit(item, GitResetMode.hard);
    }
  }

  Future<void> _checkoutCommit(GitHistoryItem item) async {
    final confirmed = await showGitCheckoutCommitConfirmation(context, item);
    if (!confirmed || !mounted) {
      return;
    }
    await _runCommitMutation(
      (notifier) => notifier.checkoutCommit(item.id),
      'Checked out ${gitHistoryItemShortId(item)}',
    );
  }

  Future<void> _revertCommit(GitHistoryItem item) async {
    final confirmed = await showGitRevertCommitConfirmation(context, item);
    if (!confirmed || !mounted) {
      return;
    }
    // First parent is the mainline convention for merge reverts.
    await _runCommitMutation(
      (notifier) => notifier.revertCommit(
        item.id,
        mainlineParent: item.parentIds.length > 1 ? 1 : null,
      ),
      'Reverted ${gitHistoryItemShortId(item)}',
    );
  }

  Future<void> _resetToCommit(GitHistoryItem item, GitResetMode mode) async {
    final confirmed = await showGitResetToCommitConfirmation(
      context,
      item,
      mode,
    );
    if (!confirmed || !mounted) {
      return;
    }
    await _runCommitMutation(
      (notifier) => notifier.resetToCommit(item.id, mode: mode),
      'Reset to ${gitHistoryItemShortId(item)}',
    );
  }

  Future<void> _runCommitMutation(
    Future<void> Function(WorkspaceSourceControlController notifier) action,
    String successMessage,
  ) async {
    try {
      await _withSourceControl(action);
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: successMessage, tone: .success);
      _compareCache.clear();
      await _reload();
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: error.toString(), tone: .error);
      }
    }
  }

  // The source-control controller is auto-disposed when nobody listens, and
  // this tab may run without the source-control panel mounted. A manual
  // subscription keeps the notifier alive for the duration of the call.
  Future<T> _withSourceControl<T>(
    Future<T> Function(WorkspaceSourceControlController notifier) action,
  ) async {
    final provider = workspaceSourceControlControllerProvider(
      _sourceControlScope.path,
    );
    final subscription = ref.listenManual(provider, (_, _) {});
    try {
      return await action(ref.read(provider.notifier));
    } finally {
      subscription.close();
    }
  }

  Future<void> _openRefMenu(
    GitHistoryItemRef itemRef,
    Offset globalPosition,
  ) async {
    final isCurrentBranch = _currentRef?.id == itemRef.id;
    final isCurrentUpstream = gitHistoryRefMatchesUpstream(
      itemRef,
      remoteRef: _remoteRef,
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
    await runGitHistoryRefMenuAction(
      context: context,
      backend: ref.read(gitBackendProvider),
      path: _sourceControlScope.path,
      itemRef: itemRef,
      action: action,
      isCurrentBranch: isCurrentBranch,
      isCurrentUpstream: isCurrentUpstream,
      currentBranch: _currentRef?.name,
      onSwitchBranch: (_) => _switchToRef(itemRef),
      onCopyName: _copyCommitText,
      onPull: () => _withSourceControl((notifier) => notifier.pull()),
      onMutationSuccess: _refreshAfterHistoryMutation,
      errorMessage: (error) => error.toString(),
    );
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
      await _withSourceControl((notifier) => notifier.refresh());
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
