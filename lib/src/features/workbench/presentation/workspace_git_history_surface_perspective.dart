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
      final selectedBranchRef = _selectedRef == _headRef ? null : _selectedRef;
      final currentBranchRef = currentBranch == _headRef ? null : currentBranch;
      final mergedTargetRef = _mergedIntoRef == _headRef
          ? null
          : _mergedIntoRef;
      final values = <String>{
        ...branches,
        ?currentBranchRef,
        ?selectedBranchRef,
        ?mergedTargetRef,
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
    if (_selectedRef == _headRef) return _headRef;
    return _selectedRef ?? _currentBranch ?? 'Current Branch';
  }

  Future<void> _setPerspective(String value) async {
    final allBranches = value == _allBranchesPerspective;
    final selectedRef = allBranches
        ? null
        : value == _headPerspective
        ? _headRef
        : value == _currentBranch
        ? null
        : value;
    if (allBranches == _allBranches && selectedRef == _selectedRef) return;

    _setSurfaceState(() {
      _allBranches = allBranches;
      _selectedRef = selectedRef;
    });
    await _refreshMergedBranchState();
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
    return _GitHistoryBranchPerspectiveMenu(
      label: _perspectiveLabel,
      branches: _branches,
      currentBranch: _currentBranch,
      allBranches: _allBranches,
      selectedRef: _selectedRef,
      onSelected: (value) => unawaited(_setPerspective(value)),
    );
  }
}

class _GitHistoryBranchPerspectiveMenu extends StatefulWidget {
  const _GitHistoryBranchPerspectiveMenu({
    required this.label,
    required this.branches,
    required this.currentBranch,
    required this.allBranches,
    required this.selectedRef,
    required this.onSelected,
  });

  final String label;
  final List<String> branches;
  final String? currentBranch;
  final bool allBranches;
  final String? selectedRef;
  final ValueChanged<String> onSelected;

  @override
  State<_GitHistoryBranchPerspectiveMenu> createState() =>
      _GitHistoryBranchPerspectiveMenuState();
}

class _GitHistoryBranchPerspectiveMenuState
    extends State<_GitHistoryBranchPerspectiveMenu> {
  final MenuController _menuController = MenuController();
  final TextEditingController _searchController = TextEditingController();
  final FocusNode _searchFocusNode = FocusNode();
  String _query = '';

  @override
  void dispose() {
    _searchController.dispose();
    _searchFocusNode.dispose();
    super.dispose();
  }

  void _open() {
    _searchController.clear();
    if (_query.isNotEmpty) setState(() => _query = '');
    _menuController.open();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _searchFocusNode.requestFocus();
    });
  }

  void _select(String value) {
    _menuController.close();
    widget.onSelected(value);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final normalizedQuery = _query.trim().toLowerCase();
    final localizedAllBranches = context.tr('All Branches').toLowerCase();
    final showHead =
        normalizedQuery.isEmpty ||
        _headRef.toLowerCase().contains(normalizedQuery);
    final filteredBranches = widget.branches
        .where(
          (branch) =>
              normalizedQuery.isEmpty ||
              branch.toLowerCase().contains(normalizedQuery),
        )
        .toList(growable: false);
    final showAllBranches =
        normalizedQuery.isEmpty ||
        'all branches'.contains(normalizedQuery) ||
        localizedAllBranches.contains(normalizedQuery);

    return MenuAnchor(
      controller: _menuController,
      menuChildren: <Widget>[
        SizedBox(
          width: 320,
          height: 320,
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space8),
            child: Column(
              crossAxisAlignment: .stretch,
              children: <Widget>[
                TextField(
                  key: const ValueKey<String>(
                    'git-history-branch-perspective-search',
                  ),
                  controller: _searchController,
                  focusNode: _searchFocusNode,
                  textInputAction: .search,
                  decoration: InputDecoration(
                    isDense: true,
                    hintText: context.tr('Search branches'),
                    prefixIcon: const Icon(Icons.search, size: 16),
                    border: const OutlineInputBorder(),
                  ),
                  onChanged: (value) => setState(() => _query = value),
                ),
                const SizedBox(height: AleraTokens.space8),
                Expanded(
                  child: ListView(
                    primary: false,
                    padding: EdgeInsets.zero,
                    children: <Widget>[
                      if (showHead)
                        MenuItemButton(
                          key: const ValueKey<String>(
                            'git-history-head-perspective',
                          ),
                          onPressed: () => _select(_headPerspective),
                          leadingIcon:
                              (!widget.allBranches &&
                                  widget.selectedRef == _headRef)
                              ? const Icon(Icons.check, size: 16)
                              : const SizedBox(width: 16),
                          child: AleraSearchHighlightedText(
                            text: _headRef,
                            query: _query,
                          ),
                        ),
                      if (showAllBranches)
                        MenuItemButton(
                          onPressed: () => _select(_allBranchesPerspective),
                          leadingIcon: widget.allBranches
                              ? const Icon(Icons.check, size: 16)
                              : const SizedBox(width: 16),
                          child: AleraSearchHighlightedText(
                            text: context.tr('All Branches'),
                            query: _query,
                          ),
                        ),
                      for (final branch in filteredBranches)
                        MenuItemButton(
                          onPressed: () => _select(branch),
                          leadingIcon:
                              (!widget.allBranches &&
                                  (widget.selectedRef == branch ||
                                      (widget.selectedRef == null &&
                                          branch == widget.currentBranch)))
                              ? const Icon(Icons.check, size: 16)
                              : const SizedBox(width: 16),
                          child: Tooltip(
                            message: branch,
                            child: Align(
                              alignment: .centerLeft,
                              child: AleraSearchHighlightedText(
                                text: branch == widget.currentBranch
                                    ? '$branch (${context.tr('Current')})'
                                    : branch,
                                query: _query,
                                maxLines: 1,
                                overflow: .ellipsis,
                              ),
                            ),
                          ),
                        ),
                      if (!showHead &&
                          !showAllBranches &&
                          filteredBranches.isEmpty)
                        Padding(
                          padding: const EdgeInsets.all(AleraTokens.space12),
                          child: Text(
                            context.tr('No matching branches'),
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: AleraTokens.foregroundMuted,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ],
      builder: (context, controller, child) => Tooltip(
        message: context.tr('Branch Perspective'),
        child: InkWell(
          key: const ValueKey<String>('git-history-branch-perspective-menu'),
          borderRadius: BorderRadius.circular(AleraTokens.radiusSm),
          onTap: controller.isOpen ? controller.close : _open,
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: AleraTokens.space4,
              vertical: AleraTokens.space2,
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 220),
                  child: Text(
                    context.tr(widget.label),
                    maxLines: 1,
                    overflow: .ellipsis,
                    style: theme.textTheme.labelMedium?.copyWith(
                      color: AleraTokens.foregroundMuted,
                    ),
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
          ),
        ),
      ),
    );
  }
}
