part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistoryFavorites on _WorkspaceGitHistorySurfaceState {
  String get _favoriteScopeKey => branchFavoritesScopeKey(
    widget.workspace.projectId,
    _sourceControlScope.relativeRoot,
  );

  Set<String> get _favoriteBranches =>
      ref
          .read(workbenchControllerProvider)
          .viewPrefs
          .favoriteBranchesByRepository[_favoriteScopeKey] ??
      const {};

  List<String>? get _includedRefs => _favoritesOnly
      ? favoriteBranchesFirst(
          _branches.where(_favoriteBranches.contains),
          _favoriteBranches,
        )
      : widget.tab.gitHistoryFilePath != null && !_allBranches
      ? <String>[_selectedRef ?? _currentBranch ?? _headRef]
      : null;

  void _toggleFavorite(String branch) {
    ref
        .read(workbenchControllerProvider.notifier)
        .toggleBranchFavorite(scopeKey: _favoriteScopeKey, branch: branch);
  }
}
