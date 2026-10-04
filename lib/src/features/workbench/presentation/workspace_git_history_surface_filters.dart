part of 'workspace_git_history_surface.dart';

/// Flattens every loaded commit reachable from [targetRevision] onto one
/// presentation lane while preserving the commits themselves.
///
/// This intentionally changes only the display topology. Merged side-branch
/// commits remain in history, but they are chained in the same order as the
/// loaded history so hiding their branch graph cannot create replacement
/// lanes, duplicate dots, or multi-parent fan-out deeper in the graph.
List<GitHistoryItem> collapseGitHistoryOntoTargetLane(
  List<GitHistoryItem> source, {
  required String targetRevision,
}) {
  if (source.isEmpty || targetRevision.isEmpty) return source;

  final byId = <String, GitHistoryItem>{
    for (final item in source) item.id: item,
  };
  final targetReachable = <String>{};
  final stack = <String>[targetRevision];
  while (stack.isNotEmpty) {
    final id = stack.removeLast();
    if (!targetReachable.add(id)) continue;
    final item = byId[id];
    if (item != null) stack.addAll(item.parentIds);
  }

  final targetItems = source
      .where((item) => targetReachable.contains(item.id))
      .toList(growable: false);
  if (targetItems.isEmpty) return source;

  final nextTargetById = <String, String>{};
  for (var index = 0; index + 1 < targetItems.length; index += 1) {
    nextTargetById[targetItems[index].id] = targetItems[index + 1].id;
  }

  return source
      .map((item) {
        if (!targetReachable.contains(item.id)) return item;
        final nextTarget = nextTargetById[item.id];
        final parentIds = nextTarget != null
            ? <String>[nextTarget]
            : item.parentIds.isEmpty
            ? const <String>[]
            : <String>[item.parentIds.first];
        return GitHistoryItem(
          id: item.id,
          parentIds: parentIds,
          subject: item.subject,
          message: item.message,
          displayId: item.displayId,
          author: item.author,
          authorEmail: item.authorEmail,
          timestamp: item.timestamp,
          references: item.references,
        );
      })
      .toList(growable: false);
}

extension _WorkspaceGitHistoryMergedFilters
    on _WorkspaceGitHistorySurfaceState {
  Future<void> _initializeHistory() async {
    await _loadBranchPerspectives();
    await _refreshMergedBranchState();
    if (mounted) {
      await _reload();
    }
  }

  String get _mergedIntoLabel =>
      _mergedIntoRef ?? _currentBranch ?? 'Current Branch';

  String get _mergedVisibilityLabel => switch (_mergedBranchVisibility) {
    WorkspaceGitHistoryMergedBranchVisibility.showAll => 'Show All',
    WorkspaceGitHistoryMergedBranchVisibility.hideMergedNames =>
      'Hide Merged Names',
    WorkspaceGitHistoryMergedBranchVisibility.hideMergedGraph =>
      'Hide Merged Graph',
  };

  Future<void> _setMergedBranchVisibility(
    WorkspaceGitHistoryMergedBranchVisibility visibility,
  ) async {
    if (visibility == _mergedBranchVisibility) return;
    _setSurfaceState(() => _mergedBranchVisibility = visibility);
    await _refreshMergedBranchState();
    _reprojectMergedBranchFilter();
    unawaited(_persistMergedBranchFilter());
  }

  Future<void> _setMergedIntoRef(String? mergedIntoRef) async {
    final normalized = mergedIntoRef == _currentBranch ? null : mergedIntoRef;
    if (normalized == _mergedIntoRef) return;
    _setSurfaceState(() => _mergedIntoRef = normalized);
    await _refreshMergedBranchState();
    _reprojectMergedBranchFilter();
    unawaited(_persistMergedBranchFilter());
  }

  Future<void> _persistMergedBranchFilter() async {
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .setGitHistoryMergedBranchFilter(
            tabId: widget.tab.id,
            visibility: _mergedBranchVisibility,
            mergedIntoRef: _mergedIntoRef,
          );
    } catch (_) {
      // The active graph already reflects the selected filter. Persistence
      // failure only means the preference is not restored next time.
    }
  }

  Future<void> _refreshMergedBranchState() async {
    final visibility = _mergedBranchVisibility;
    final target = _mergedIntoRef ?? _currentBranch;
    if (!mounted) return;
    if (!_allBranches ||
        visibility == WorkspaceGitHistoryMergedBranchVisibility.showAll ||
        target == null ||
        target.trim().isEmpty) {
      _setSurfaceState(() {
        _mergedBranches = const <String>{};
        _mergedIntoRevision = null;
      });
      return;
    }

    final backend = ref.read(gitBackendProvider);
    final path = _sourceControlScope.path;
    String? targetRevision;
    try {
      final targetHistory = await backend.history(
        path,
        limit: 1,
        baseRef: target,
      );
      final candidates = <GitHistoryItemRef?>[
        targetHistory.currentRef,
        targetHistory.remoteRef,
        targetHistory.baseRef,
      ];
      for (final candidate in candidates) {
        if (candidate == null) continue;
        if (candidate.name == target || candidate.id == target) {
          targetRevision = candidate.revision;
          break;
        }
      }
      targetRevision ??= target == _currentBranch
          ? targetHistory.currentRef?.revision
          : targetHistory.baseRef?.revision;
    } catch (_) {
      // Name-only filtering remains useful even if resolving the target OID
      // fails; graph filtering will gracefully fall back to hiding labels.
    }

    final merged = <String>{};
    final candidates = _branches
        .where((branch) => branch != target)
        .toList(growable: false);
    const batchSize = 16;
    for (var start = 0; start < candidates.length; start += batchSize) {
      final end = math.min(start + batchSize, candidates.length);
      final batch = candidates.sublist(start, end);
      final results = await Future.wait(
        batch.map((branch) async {
          try {
            final isMerged = await backend.isAncestor(
              path: path,
              ancestorRef: branch,
              descendantRef: target,
            );
            return (branch: branch, isMerged: isMerged);
          } catch (_) {
            return (branch: branch, isMerged: false);
          }
        }),
      );
      for (final result in results) {
        if (result.isMerged) merged.add(result.branch);
      }
      if (!mounted ||
          visibility != _mergedBranchVisibility ||
          target != (_mergedIntoRef ?? _currentBranch)) {
        return;
      }
    }

    _setSurfaceState(() {
      _mergedBranches = Set<String>.unmodifiable(merged);
      _mergedIntoRevision = targetRevision;
    });
  }

  void _reprojectMergedBranchFilter() {
    if (!mounted || _items.isEmpty) return;
    _setSurfaceState(() {
      final projection = _buildProjection(_filteredHistoryItems(_items));
      _viewModels = projection.viewModels;
      _projectionContinuation = projection.continuation;
    });
  }

  List<GitHistoryItem> _filteredHistoryItems(List<GitHistoryItem> source) {
    if (!_allBranches ||
        _mergedBranchVisibility ==
            WorkspaceGitHistoryMergedBranchVisibility.showAll) {
      return source;
    }

    GitHistoryItem withoutMergedRefs(GitHistoryItem item) {
      final refs = item.references
          .where(
            (itemRef) =>
                !_isBranchRef(itemRef) ||
                !_mergedBranches.contains(itemRef.name),
          )
          .toList(growable: false);
      return refs.length == item.references.length
          ? item
          : item.copyWith(references: refs);
    }

    if (_mergedBranchVisibility ==
        WorkspaceGitHistoryMergedBranchVisibility.hideMergedNames) {
      return source.map(withoutMergedRefs).toList(growable: false);
    }

    final targetRevision = _mergedIntoRevision;
    if (targetRevision == null || targetRevision.isEmpty) {
      return source.map(withoutMergedRefs).toList(growable: false);
    }

    return collapseGitHistoryOntoTargetLane(
      source.map(withoutMergedRefs).toList(growable: false),
      targetRevision: targetRevision,
    );
  }

  static bool _isBranchRef(GitHistoryItemRef itemRef) =>
      itemRef.category == GitHistoryRefCategory.branches ||
      itemRef.category == GitHistoryRefCategory.remoteBranches ||
      itemRef.id.startsWith('refs/heads/') ||
      itemRef.id.startsWith('refs/remotes/');

  Widget _buildMergedVisibilityMenu(ThemeData theme) {
    return MenuAnchor(
      menuChildren: <Widget>[
        for (final visibility
            in WorkspaceGitHistoryMergedBranchVisibility.values)
          MenuItemButton(
            key: ValueKey<String>('git-history-merged-${visibility.key}'),
            leadingIcon: visibility == _mergedBranchVisibility
                ? const Icon(Icons.check, size: 16)
                : const SizedBox(width: 16),
            onPressed: () => unawaited(_setMergedBranchVisibility(visibility)),
            child: Text(switch (visibility) {
              WorkspaceGitHistoryMergedBranchVisibility.showAll => context.tr(
                'Show All',
              ),
              WorkspaceGitHistoryMergedBranchVisibility.hideMergedNames =>
                context.tr('Hide Merged Names'),
              WorkspaceGitHistoryMergedBranchVisibility.hideMergedGraph =>
                context.tr('Hide Merged Graph'),
            }),
          ),
      ],
      builder: (context, controller, child) => Tooltip(
        message: context.tr('Merged Branch Visibility'),
        child: InkWell(
          key: const ValueKey<String>('git-history-merged-visibility-menu'),
          borderRadius: BorderRadius.circular(AleraTokens.radiusSm),
          onTap: controller.isOpen ? controller.close : controller.open,
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: AleraTokens.space4,
              vertical: AleraTokens.space2,
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Text(
                  context.tr(_mergedVisibilityLabel),
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
          ),
        ),
      ),
    );
  }

  Widget _buildMergedTargetMenu(ThemeData theme) {
    return _GitHistoryMergedTargetMenu(
      label: _mergedIntoLabel,
      branches: _branches,
      currentBranch: _currentBranch,
      selectedRef: _mergedIntoRef,
      onSelected: (value) => unawaited(_setMergedIntoRef(value)),
    );
  }
}

class _GitHistoryMergedTargetMenu extends StatefulWidget {
  const _GitHistoryMergedTargetMenu({
    required this.label,
    required this.branches,
    required this.currentBranch,
    required this.selectedRef,
    required this.onSelected,
  });

  final String label;
  final List<String> branches;
  final String? currentBranch;
  final String? selectedRef;
  final ValueChanged<String?> onSelected;

  @override
  State<_GitHistoryMergedTargetMenu> createState() =>
      _GitHistoryMergedTargetMenuState();
}

class _GitHistoryMergedTargetMenuState
    extends State<_GitHistoryMergedTargetMenu> {
  final MenuController _controller = MenuController();
  final TextEditingController _search = TextEditingController();
  final FocusNode _focus = FocusNode();
  String _query = '';

  @override
  void dispose() {
    _search.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _open() {
    _search.clear();
    if (_query.isNotEmpty) setState(() => _query = '');
    _controller.open();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focus.requestFocus();
    });
  }

  void _select(String? value) {
    _controller.close();
    widget.onSelected(value);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final query = _query.trim().toLowerCase();
    final branches = widget.branches
        .where(
          (branch) => query.isEmpty || branch.toLowerCase().contains(query),
        )
        .toList(growable: false);
    return MenuAnchor(
      controller: _controller,
      menuChildren: <Widget>[
        SizedBox(
          width: 280,
          height: 300,
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space8),
            child: Column(
              children: <Widget>[
                TextField(
                  key: const ValueKey<String>(
                    'git-history-merged-target-search',
                  ),
                  controller: _search,
                  focusNode: _focus,
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
                      if (query.isEmpty ||
                          context
                              .tr('Current Branch')
                              .toLowerCase()
                              .contains(query))
                        MenuItemButton(
                          key: const ValueKey<String>(
                            'git-history-merged-target-current',
                          ),
                          leadingIcon: widget.selectedRef == null
                              ? const Icon(Icons.check, size: 16)
                              : const SizedBox(width: 16),
                          onPressed: () => _select(null),
                          child: Text(
                            widget.currentBranch == null
                                ? context.tr('Current Branch')
                                : '${widget.currentBranch} (${context.tr('Current')})',
                          ),
                        ),
                      for (final branch in branches)
                        if (branch != widget.currentBranch)
                          MenuItemButton(
                            key: ValueKey<String>(
                              'git-history-merged-target-$branch',
                            ),
                            onPressed: () => _select(branch),
                            leadingIcon: widget.selectedRef == branch
                                ? const Icon(Icons.check, size: 16)
                                : const SizedBox(width: 16),
                            child: Tooltip(
                              message: branch,
                              child: Align(
                                alignment: Alignment.centerLeft,
                                child: Text(
                                  branch,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
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
        message: context.tr('Merged Into'),
        child: InkWell(
          key: const ValueKey<String>('git-history-merged-target-menu'),
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
                Text(
                  '${context.tr('Merged Into')}: ${widget.label}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
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
          ),
        ),
      ),
    );
  }
}
