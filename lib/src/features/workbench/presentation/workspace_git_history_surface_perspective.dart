part of 'workspace_git_history_surface.dart';

extension _WorkspaceGitHistoryPerspective on _WorkspaceGitHistorySurfaceState {
  Future<void> _loadBranchPerspectives() async {
    try {
      final backend = ref.read(gitBackendProvider);
      final currentBranch = await backend.currentBranch(
        _sourceControlScope.path,
      );
      final branches = await backend.listBranches(_sourceControlScope.path);
      if (!mounted) return;
      final values = <String>{
        ...branches,
        currentBranch,
        ?_selectedRef,
      }.toList()..sort();
      _setSurfaceState(() {
        _currentBranch = currentBranch;
        _branches = values;
      });
    } catch (_) {
      // History remains usable even if branch discovery is unavailable.
    }
  }

  String get _perspectiveLabel {
    if (_allBranches) return 'All Branches';
    return _selectedRef ?? _currentBranch ?? 'Current Branch';
  }

  Future<void> _setPerspective(String value) async {
    final allBranches = value == _allBranchesPerspective;
    final selectedRef = allBranches || value == _currentBranch ? null : value;
    if (allBranches == _allBranches && selectedRef == _selectedRef) return;

    _setSurfaceState(() {
      _allBranches = allBranches;
      _selectedRef = selectedRef;
    });
    unawaited(_reload());
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .setGitHistoryAllBranches(
            tabId: widget.tab.id,
            allBranches: allBranches,
            selectedRef: selectedRef,
          );
    } catch (_) {
      // Persistence failures only lose the saved preference; the current
      // view already switched and stays correct for this session.
    }
  }

  Widget _buildBranchPerspectiveMenu(ThemeData theme) {
    return PopupMenuButton<String>(
      tooltip: 'Branch Perspective',
      onSelected: (value) => unawaited(_setPerspective(value)),
      itemBuilder: (context) => <PopupMenuEntry<String>>[
        const PopupMenuItem<String>(
          value: _allBranchesPerspective,
          child: Text('All Branches'),
        ),
        for (final branch in _branches)
          PopupMenuItem<String>(
            value: branch,
            child: Text(
              branch == _currentBranch ? '$branch (Current)' : branch,
            ),
          ),
      ],
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            _perspectiveLabel,
            style: theme.textTheme.labelMedium?.copyWith(
              color: AleraTokens.foregroundMuted,
            ),
          ),
          const SizedBox(width: AleraTokens.space4),
          const Icon(
            AleraIcons.chevronDown,
            size: 14,
            color: AleraTokens.foregroundMuted,
          ),
        ],
      ),
    );
  }
}
