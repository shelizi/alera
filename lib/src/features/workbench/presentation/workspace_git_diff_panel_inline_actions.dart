part of 'workspace_git_diff_panel.dart';

class const _GitFileActions({
  required final GitChangeEntry entry,
  required final bool busy,
  required final ValueChanged<GitChangeEntry> onStage,
  required final ValueChanged<GitChangeEntry> onUnstage,
  required final ValueChanged<GitChangeEntry> onDiscard,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 58,
      child: Row(
        mainAxisAlignment: .end,
        children: <Widget>[
          if (entry.canUnstageFromParent)
            AleraIconButton(
              tooltip: 'Unstage',
              icon: AleraIcons.gitUnstage,
              onPressed: busy ? null : () => onUnstage(entry),
            )
          else if (entry.canStageFromParent)
            AleraIconButton(
              tooltip: 'Stage',
              icon: AleraIcons.gitStage,
              onPressed: busy ? null : () => onStage(entry),
            ),
          if (entry.canDiscardFromParent)
            AleraIconButton(
              tooltip: 'Discard',
              icon: AleraIcons.gitDiscard,
              onPressed: busy ? null : () => onDiscard(entry),
            ),
        ],
      ),
    );
  }
}

class const _AreaActions({
  required final bool busy,
  required final VoidCallback onStage,
  required final VoidCallback onUnstage,
  required final VoidCallback onDiscard,
  required final bool canStage,
  required final bool canUnstage,
  required final bool canDiscard,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 58,
      child: Row(
        mainAxisAlignment: .end,
        children: <Widget>[
          if (canUnstage)
            AleraIconButton(
              tooltip: 'Unstage',
              icon: AleraIcons.gitUnstage,
              onPressed: busy ? null : onUnstage,
            )
          else if (canStage)
            AleraIconButton(
              tooltip: 'Stage',
              icon: AleraIcons.gitStage,
              onPressed: busy ? null : onStage,
            ),
          if (canDiscard)
            AleraIconButton(
              tooltip: 'Discard',
              icon: AleraIcons.gitDiscard,
              onPressed: busy ? null : onDiscard,
            ),
        ],
      ),
    );
  }
}

extension _WorkspaceGitDiffPanelActions on _WorkspaceGitDiffPanelState {
  void _toggleSubmodule(GitChangeEntry entry) {
    _setPanelState(() {
      if (!_expandedSubmodules.remove(entry.id)) {
        _expandedSubmodules.add(entry.id);
      }
    });
  }

  Future<void> _refresh() {
    return _run(
      () => _notifier.refresh(),
      successMessage: 'Source control refreshed',
    );
  }

  Future<void> _runToolbarAction(_SourceControlMenuAction action) async {
    if (_actionRequiresMessage(action)) {
      await _commitAction(action);
      return;
    }
    await _handleMenuAction(action);
  }

  Future<void> _handleMenuAction(_SourceControlMenuAction action) async {
    switch (action) {
      case _SourceControlMenuAction.refresh:
        await _refresh();
      case _SourceControlMenuAction.commit:
      case _SourceControlMenuAction.commitPush:
      case _SourceControlMenuAction.commitSync:
        await _commitAction(action);
      case _SourceControlMenuAction.amend:
        await _amendAction();
      case _SourceControlMenuAction.stageAll:
        await _stage(null);
      case _SourceControlMenuAction.unstageAll:
        await _run(() => _notifier.unstage(null), successMessage: 'Unstaged');
      case _SourceControlMenuAction.discardAll:
        await _discard(null);
      case _SourceControlMenuAction.fetch:
        await _run(() => _notifier.fetch(), successMessage: 'Fetched');
      case _SourceControlMenuAction.pull:
        await _run(() => _notifier.pull(), successMessage: 'Pulled');
      case _SourceControlMenuAction.push:
        await _run(() => _notifier.push(), successMessage: 'Pushed');
      case _SourceControlMenuAction.publishBranch:
        await _run(() => _notifier.push(), successMessage: 'Branch published');
      case _SourceControlMenuAction.sync:
        await _run(() => _notifier.sync(), successMessage: 'Synced');
      case _SourceControlMenuAction.stash:
        await _run(() => _notifier.stash(), successMessage: 'Stashed');
      case _SourceControlMenuAction.stashPop:
        final stash = await _pickStash();
        if (stash == null) {
          return;
        }
        await _run(
          () => _notifier.stashPop(stash.index),
          successMessage: 'Stash popped',
        );
      case _SourceControlMenuAction.openChangesInZed:
        await _openChangesInZed(
          ref
              .read(
                workspaceSourceControlControllerProvider(
                  widget.sourceControlScope.path,
                ),
              )
              .asData
              ?.value,
        );
    }
  }

  Future<void> _stage(String? filePath) {
    return _run(() => _notifier.stage(filePath), successMessage: 'Staged');
  }

  Future<void> _unstage(String? filePath) {
    return _run(() => _notifier.unstage(filePath), successMessage: 'Unstaged');
  }

  Future<void> _stageArea(GitChangeArea area, String? filePath) {
    return _run(
      () => _notifier.stageArea(area, filePath: filePath),
      successMessage: 'Staged',
    );
  }

  Future<void> _stageEntry(GitChangeEntry entry) {
    return _run(() => _notifier.stageEntry(entry), successMessage: 'Staged');
  }

  Future<void> _unstageEntry(GitChangeEntry entry) {
    return _run(
      () => _notifier.unstageEntry(entry),
      successMessage: 'Unstaged',
    );
  }

  Future<void> _unstageArea(GitChangeArea area, String? filePath) {
    return _run(
      () => _notifier.unstageArea(area, filePath: filePath),
      successMessage: 'Unstaged',
    );
  }

  Future<void> _discard(String? filePath) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => AleraConfirmDialog(
        title: filePath == null ? 'Discard All Changes?' : 'Discard Changes?',
        message: filePath == null
            ? 'This permanently discards unstaged and untracked changes in this workspace.'
            : 'This permanently discards unstaged and untracked changes in "$filePath".',
        confirmLabel: 'Discard',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }
    await _run(
      () => _notifier.discard(filePath),
      successMessage: filePath == null
          ? 'Changes discarded'
          : 'Change discarded',
    );
  }

  Future<void> _discardAreaWithConfirmation(
    GitChangeArea area,
    String? filePath,
  ) async {
    final target = filePath ?? area.label.toLowerCase();
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => AleraConfirmDialog(
        title: 'Discard Changes?',
        message: 'This permanently discards changes in "$target".',
        confirmLabel: 'Discard',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }
    await _run(
      () => _notifier.discardArea(area, filePath: filePath),
      successMessage: 'Changes discarded',
    );
  }

  Future<void> _discardEntry(GitChangeEntry entry) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => AleraConfirmDialog(
        title: 'Discard Changes?',
        message:
            'This permanently discards unstaged and untracked changes in "${entry.path}".',
        confirmLabel: 'Discard',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }
    await _run(
      () => _notifier.discardEntry(entry),
      successMessage: 'Change discarded',
    );
  }

  Future<bool> _run(
    Future<void> Function() action, {
    required String successMessage,
  }) async {
    try {
      await action();
      if (mounted) {
        AleraToast.show(context, message: successMessage, tone: .success);
        _invalidateGitHistoryAfterMutation();
      }
      return true;
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
      return false;
    }
  }

  Future<void> _checkoutCommit(GitHistoryItem item) async {
    final confirmed = await showGitCheckoutCommitConfirmation(context, item);
    if (!confirmed || !mounted) {
      return;
    }
    await _run(
      () => _notifier.checkoutCommit(item.id),
      successMessage: 'Checked out ${gitHistoryItemShortId(item)}',
    );
  }

  Future<void> _revertCommit(GitHistoryItem item) async {
    final confirmed = await showGitRevertCommitConfirmation(context, item);
    if (!confirmed || !mounted) {
      return;
    }
    // First parent is the mainline convention for merge reverts.
    await _run(
      () => _notifier.revertCommit(
        item.id,
        mainlineParent: item.parentIds.length > 1 ? 1 : null,
      ),
      successMessage: 'Reverted ${gitHistoryItemShortId(item)}',
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
    await _run(
      () => _notifier.resetToCommit(item.id, mode: mode),
      successMessage: 'Reset to ${gitHistoryItemShortId(item)}',
    );
  }

  Future<void> _switchBranch(String branch) async {
    final callback = widget.onSwitchBranch;
    if (callback == null) {
      return;
    }
    await _run(() async {
      await callback(branch);
      await _notifier.refresh();
    }, successMessage: 'Switched to $branch');
  }

  Future<void> _handleGitHistoryRefAction(
    GitHistoryItemRef itemRef,
    GitHistoryRefMenuAction action, {
    required bool isCurrentBranch,
    required bool isCurrentUpstream,
    required String? currentBranch,
  }) {
    return runGitHistoryRefMenuAction(
      context: context,
      backend: ref.read(gitBackendProvider),
      path: widget.sourceControlScope.path,
      itemRef: itemRef,
      action: action,
      isCurrentBranch: isCurrentBranch,
      isCurrentUpstream: isCurrentUpstream,
      currentBranch: currentBranch,
      onSwitchBranch: widget.onSwitchBranch == null ? null : _switchBranch,
      onCopyName: _copyCommitText,
      onPull: () => _notifier.pull(),
      onMutationSuccess: _refreshAfterHistoryMutation,
      onCreateWorktree: _createWorktreeFromHistory,
      errorMessage: _messageFor,
    );
  }

  /// Creates a linked-worktree workspace through the catalog owner so the
  /// history panel follows the same provisioning path as the sidebar flow.
  Future<void> _createWorktreeFromHistory({
    required String sourceBranch,
    required String newBranchName,
    required bool reuseExistingBranch,
  }) async {
    final project = ref
        .read(workbenchControllerProvider)
        .projects
        .where((candidate) => candidate.id == widget.workspace.projectId)
        .firstOrNull;
    if (project == null) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not create a worktree: project not found',
          tone: .error,
        );
      }
      return;
    }
    await ref
        .read(workbenchControllerProvider.notifier)
        .createWorkspace(
          project: project,
          sourceBranch: sourceBranch,
          newBranchName: newBranchName,
          reuseExistingBranch: reuseExistingBranch,
          name: newBranchName,
        );
    if (!mounted) {
      return;
    }
    AleraToast.show(
      context,
      message: 'Created workspace $newBranchName',
      tone: .success,
    );
  }

  Future<void> _handleGitHistoryBoundaryAction(
    GitHistoryBoundaryMenuAction action,
  ) {
    return runGitHistoryBoundaryMenuAction(
      context: context,
      action: action,
      onStash: () => _notifier.stash(),
      onDiscardAll: _discardAllHistoryChanges,
      onCommitChanges: _focusCommitMessage,
      onMutationSuccess: _refreshAfterHistoryMutation,
      errorMessage: _messageFor,
    );
  }

  Future<void> _discardAllHistoryChanges() async {
    await _notifier.discard(null);
    if (!mounted) {
      return;
    }
    await _notifier.discardArea(GitChangeArea.staged);
  }

  Future<void> _refreshAfterHistoryMutation() async {
    if (!mounted) {
      return;
    }
    await _notifier.refresh();
    if (!mounted) {
      return;
    }
    await _refreshGitHistory();
  }

  void _focusCommitMessage() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _messageFocusNode.requestFocus();
      }
    });
  }

  Future<GitStashEntry?> _pickStash() {
    final current = ref
        .read(
          workspaceSourceControlControllerProvider(
            widget.sourceControlScope.path,
          ),
        )
        .asData
        ?.value;
    final stashes = current?.stashes ?? const <GitStashEntry>[];
    if (stashes.isEmpty) {
      AleraToast.show(context, message: 'No stashes to pop');
      return Future<GitStashEntry?>.value();
    }
    return showDialog<GitStashEntry>(
      context: context,
      builder: (_) => _StashPickerDialog(stashes: stashes),
    );
  }

  WorkspaceSourceControlController get _notifier => ref.read(
    workspaceSourceControlControllerProvider(widget.sourceControlScope.path)
        .notifier,
  );
}
