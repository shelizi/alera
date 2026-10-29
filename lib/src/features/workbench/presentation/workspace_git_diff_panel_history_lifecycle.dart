part of 'workspace_git_diff_panel.dart';

/// History panel lifecycle is separate from source-control mutation, file-tree
/// projection, and the panel's primary widget build.
extension _WorkspaceGitDiffPanelHistoryLifecycle
    on _WorkspaceGitDiffPanelState {
  _GitHistoryPanelLoadState get _historyPanelState {
    final result = _historyLoader.result;
    if (_historyLoader.error case final error?) {
      return _GitHistoryPanelLoadState.error(
        error: _messageFor(error),
        result: result,
        loading: _historyLoader.refreshing,
      );
    }
    if (_historyLoader.loading && result == null) {
      return const _GitHistoryPanelLoadState.loading();
    }
    if (result != null) {
      return _GitHistoryPanelLoadState.ready(
        result: result,
        loading: _historyLoader.refreshing,
      );
    }
    return const _GitHistoryPanelLoadState.idle();
  }

  void _toggleGitHistory() {
    final opening = _historyCollapsed;
    _setPanelState(() => _historyCollapsed = !_historyCollapsed);
    if (opening && _historyLoader.needsLoad) {
      unawaited(_loadGitHistory());
    }
  }

  void _invalidateGitHistoryAfterMutation() {
    _commitCompareCache.clear();
    _historyLoader.markStale();
    if (_historyCollapsed) {
      return;
    }
    unawaited(_loadGitHistory());
  }

  void _invalidateGitHistoryAfterRepositoryChange() {
    if (_historyLoader.result == null && _historyLoader.inFlight == null) {
      return;
    }
    _commitCompareCache.clear();
    _historyLoader.markStale();
    if (_historyCollapsed) {
      return;
    }
    unawaited(_loadGitHistory());
  }

  Future<void> _refreshGitHistory() async {
    if (_historyCollapsed) {
      _setPanelState(() => _historyCollapsed = false);
    }
    await _loadGitHistory();
  }

  Future<void> _loadGitHistory() async {
    _commitCompareCache.clear();
    try {
      await _historyLoader.load();
    } on Object {
      // The loader stores the error and notifies the panel; the wrapper keeps
      // fire-and-forget refreshes from producing an unhandled future error.
    }
  }

  Future<List<GitCommitChangeEntry>> _loadCommitFiles(
    GitHistoryItem item,
  ) async {
    final compare = await _commitCompareFor(item);
    return compare.entries;
  }

  Future<void> _openCommitDiff(GitHistoryItem item) async {
    try {
      final compare = await _commitCompareFor(item);
      if (!mounted) {
        return;
      }
      await widget.onOpenGitCommitDiff(
        scope: .all,
        gitDiffRoot: widget.sourceControlScope.relativeRoot,
        commitOid: compare.summary.commitOid,
        parentOid: compare.summary.parentOid,
        compareRef: compare.summary.compareRef,
        subject: item.subject,
        message: item.message,
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    }
  }

  Future<void> _openCommitGraph() async {
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .openGitHistoryTab(
            workspace: widget.workspace,
            gitDiffRoot: widget.sourceControlScope.relativeRoot,
          );
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    }
  }

  Future<GitCommitCompareResult> _commitCompareFor(GitHistoryItem item) async {
    return _commitCompareCache.compareFor(item.id);
  }

  Future<void> _copyCommitText(String text, String label) async {
    try {
      await Clipboard.setData(ClipboardData(text: text));
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: '$label copied', tone: .success);
    } catch (_) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not copy $label',
          tone: .error,
        );
      }
    }
  }
}
