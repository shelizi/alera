part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistoryBranchComparison
    on _WorkspaceGitHistorySurfaceState {
  /// Git Graph ref shortcut: open the existing pinned, per-file Diff surface
  /// immediately, using the live current branch as the comparison base.
  Future<void> _compareFilesWithCurrentBranch(String targetBranch) async {
    try {
      final snapshot = await BranchCompareCurrentService(
        ref.read(gitBackendProvider),
      ).prepare(path: _sourceControlScope.path, compareBranch: targetBranch);
      if (!mounted) return;
      await ref
          .read(workbenchControllerProvider.notifier)
          .openGitBranchCompareTab(
            workspace: widget.workspace,
            gitDiffRoot: _sourceControlScope.relativeRoot,
            baseRef: snapshot.baseRef,
            headRef: snapshot.headRef,
            headOid: snapshot.headOid,
            mergeBaseOid: snapshot.mergeBaseOid,
          );
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: gitHistoryErrorMessage(error),
          tone: .error,
        );
      }
    }
  }

  Future<void> _openBranchCompare({String? initialHeadRef}) async {
    try {
      final backend = ref.read(gitBackendProvider);
      final path = _sourceControlScope.path;
      final branches = await backend.listBranches(path);
      if (!mounted) return;
      if (branches.length < 2) {
        AleraToast.show(
          context,
          message: 'At least two Git branches are required to compare.',
          tone: .error,
        );
        return;
      }
      final selection = await showGitBranchCompareDialog(
        context: context,
        branches: branches,
        currentBranch: _currentBranch ?? 'HEAD',
        initialHeadRef: initialHeadRef,
      );
      if (selection == null || !mounted) return;
      final compare = await backend.compareRange(
        path: path,
        baseRef: selection.baseRef,
        headRef: selection.headRef,
      );
      final mergeBaseOid = compare.summary.parentOid;
      if (compare.summary.status != GitCommitCompareStatus.ready ||
          mergeBaseOid == null) {
        throw GitInternalException(
          compare.summary.errorMessage ??
              'Could not find a common Git ancestor for these branches.',
        );
      }
      if (!mounted) return;
      await ref
          .read(workbenchControllerProvider.notifier)
          .openGitBranchCompareTab(
            workspace: widget.workspace,
            gitDiffRoot: _sourceControlScope.relativeRoot,
            baseRef: selection.baseRef,
            headRef: selection.headRef,
            headOid: compare.summary.commitOid,
            mergeBaseOid: mergeBaseOid,
          );
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: gitHistoryErrorMessage(error),
          tone: .error,
        );
      }
    }
  }
}
