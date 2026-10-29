part of 'alera_shell_page.dart';

class const WorkspaceBranchStatusBarControl({
  super.key,
  required final Workspace workspace,
  required final WorkspaceSourceControlScope sourceControlScope,
  required final Future<void> Function(String branch) onSwitchBranch,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceBranchStatusBarControl> createState() =>
      _WorkspaceBranchStatusBarControlState();
}

class _WorkspaceBranchStatusBarControlState
    extends ConsumerState<WorkspaceBranchStatusBarControl> {
  final MenuController _menu = MenuController();
  final TextEditingController _search = TextEditingController();
  final FocusNode _searchFocus = FocusNode(
    debugLabel: 'workspace-branch-search',
  );
  List<String> _branches = const <String>[];
  String? _current;
  String _query = '';
  bool _loading = false;
  bool _switching = false;
  int _generation = 0;

  @override
  void initState() {
    super.initState();
    _current = _normalizedWorkspaceBranch(widget.workspace.branch);
    unawaited(_loadBranches());
  }

  @override
  void didUpdateWidget(covariant WorkspaceBranchStatusBarControl oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.sourceControlScope.path != widget.sourceControlScope.path) {
      _current = _normalizedWorkspaceBranch(widget.workspace.branch);
      _branches = const <String>[];
      unawaited(_loadBranches());
      return;
    }
    if (oldWidget.workspace.branch != widget.workspace.branch) {
      _current =
          _normalizedWorkspaceBranch(widget.workspace.branch) ?? _current;
    }
  }

  @override
  void dispose() {
    _generation += 1;
    _search.dispose();
    _searchFocus.dispose();
    super.dispose();
  }

  Future<void> _loadBranches() async {
    final generation = ++_generation;
    if (mounted) setState(() => _loading = true);
    try {
      final backend = ref.read(gitBackendProvider);
      final path = widget.sourceControlScope.path;
      final current = await backend.currentBranch(path);
      final branches = await backend.listBranches(path);
      var remotes = const <GitRemote>[];
      try {
        remotes = await backend.listRemotes(path);
      } on Object {
        // Keep local branch discovery usable when remote metadata is unavailable.
      }
      if (!mounted || generation != _generation) return;
      final localBranches = filterWorkspaceLocalBranches(
        branches: branches,
        currentBranch: current,
        remoteNames: <String>[for (final remote in remotes) remote.name],
      );
      setState(() {
        _branches = localBranches;
        _current = _normalizedWorkspaceBranch(current) ?? _current;
        _loading = false;
      });
    } on Object {
      if (mounted && generation == _generation) {
        setState(() => _loading = false);
      }
    }
  }

  void _openMenu() {
    if (_switching) return;
    _search.clear();
    if (_query.isNotEmpty) setState(() => _query = '');
    _menu.open();
    unawaited(_loadBranches());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _searchFocus.requestFocus();
    });
  }

  Future<void> _copyBranchName(String branch) async {
    await Clipboard.setData(ClipboardData(text: branch));
    if (mounted) {
      AleraToast.show(
        context,
        message: context.trTemplate('Copied branch name: {branch}', {
          'branch': branch,
        }),
      );
    }
  }

  Future<void> _showCopyBranchMenu(Offset position, String branch) async {
    final selected = await showMenu<bool>(
      context: context,
      position: RelativeRect.fromLTRB(
        position.dx,
        position.dy,
        position.dx,
        position.dy,
      ),
      items: <PopupMenuEntry<bool>>[
        PopupMenuItem<bool>(
          value: true,
          child: Text(context.tr('Copy Branch Name')),
        ),
      ],
    );
    if (selected == true && mounted) {
      await _copyBranchName(branch);
    }
  }

  void _selectBranch(String branch) {
    _menu.close();
    if (_switching || branch == _current) return;
    unawaited(_switchBranch(branch));
  }

  Future<void> _switchBranch(String branch) async {
    setState(() => _switching = true);
    try {
      await widget.onSwitchBranch(branch);
      if (mounted) setState(() => _current = branch);
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: error.toString(), tone: .error);
      }
    } finally {
      if (mounted) setState(() => _switching = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final label =
        _current ??
        _normalizedWorkspaceBranch(widget.workspace.branch) ??
        'HEAD';
    final query = _query.trim().toLowerCase();
    final filtered = _branches
        .where(
          (branch) => query.isEmpty || branch.toLowerCase().contains(query),
        )
        .toList(growable: false);
    final scopeKey = branchFavoritesScopeKey(
      widget.workspace.projectId,
      widget.sourceControlScope.relativeRoot,
    );
    final favorites = ref.watch(
      workbenchControllerProvider.select(
        (state) =>
            state.viewPrefs.favoriteBranchesByRepository[scopeKey] ??
            const <String>{},
      ),
    );
    final visibleCurrent = filtered.where((branch) => branch == _current);
    final pinned = favoriteBranchesFirst(
      filtered.where(
        (branch) => branch != _current && favorites.contains(branch),
      ),
      favorites,
    );
    final rows = <WorkspaceBranchTreeRow>[
      // The checked-out branch always leads, even when other branches are
      // favorited. Never duplicate it in the favorites or tree sections.
      for (final branch in visibleCurrent)
        WorkspaceBranchTreeRow(
          segment: branch,
          fullPath: branch,
          branch: branch,
          depth: 0,
          hasChildren: false,
        ),
      for (final branch in pinned)
        WorkspaceBranchTreeRow(
          segment: branch,
          fullPath: branch,
          branch: branch,
          depth: 0,
          hasChildren: false,
        ),
      ...buildWorkspaceBranchTreeRows(
        filtered
            .where(
              (branch) => branch != _current && !favorites.contains(branch),
            )
            .toList(),
      ),
    ];

    return MenuAnchor(
      controller: _menu,
      menuChildren: <Widget>[
        SizedBox(
          key: const ValueKey<String>('workspace-branch-menu'),
          width: 288,
          height: 300,
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space6),
            child: Column(
              children: <Widget>[
                SizedBox(
                  height: 34,
                  child: TextField(
                    key: const ValueKey<String>('workspace-branch-search'),
                    controller: _search,
                    focusNode: _searchFocus,
                    textInputAction: .search,
                    style: const TextStyle(fontSize: 12),
                    decoration: InputDecoration(
                      isDense: true,
                      hintText: context.tr('Search branches'),
                      prefixIcon: const Icon(Icons.search, size: 15),
                      prefixIconConstraints: const BoxConstraints(
                        minWidth: 30,
                        minHeight: 30,
                      ),
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: AleraTokens.space8,
                        vertical: AleraTokens.space6,
                      ),
                      border: const OutlineInputBorder(),
                    ),
                    onChanged: (value) => setState(() => _query = value),
                  ),
                ),
                const SizedBox(height: AleraTokens.space4),
                Expanded(
                  child: _loading && rows.isEmpty
                      ? const Center(
                          child: SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                        )
                      : rows.isEmpty
                      ? Center(child: Text(context.tr('No matching branches')))
                      : ListView.builder(
                          primary: false,
                          padding: EdgeInsets.zero,
                          itemCount: rows.length,
                          itemBuilder: (context, index) {
                            final row = rows[index];
                            final selected = row.branch == _current;
                            return GestureDetector(
                              behavior: HitTestBehavior.translucent,
                              onSecondaryTapDown: row.branch == null
                                  ? null
                                  : (details) => unawaited(
                                      _showCopyBranchMenu(
                                        details.globalPosition,
                                        row.branch!,
                                      ),
                                    ),
                              child: MenuItemButton(
                                key: row.branch == null
                                    ? null
                                    : ValueKey<String>(
                                        'workspace-branch-option:${row.branch}',
                                      ),
                                style: ButtonStyle(
                                  backgroundColor: selected
                                      ? WidgetStatePropertyAll(
                                          AleraTokens.accent.withValues(
                                            alpha: 0.12,
                                          ),
                                        )
                                      : null,
                                  minimumSize: const WidgetStatePropertyAll(
                                    Size(0, 32),
                                  ),
                                  maximumSize: const WidgetStatePropertyAll(
                                    Size.fromHeight(32),
                                  ),
                                  padding: const WidgetStatePropertyAll(
                                    EdgeInsets.symmetric(
                                      horizontal: AleraTokens.space4,
                                    ),
                                  ),
                                  tapTargetSize:
                                      MaterialTapTargetSize.shrinkWrap,
                                ),
                                onPressed: row.branch == null
                                    ? null
                                    : () => _selectBranch(row.branch!),
                                child: Padding(
                                  padding: EdgeInsets.only(
                                    left: row.depth * AleraTokens.space12,
                                  ),
                                  child: Row(
                                    children: <Widget>[
                                      SizedBox(
                                        width: 14,
                                        child: selected
                                            ? const Icon(
                                                AleraIcons.check,
                                                size: 14,
                                              )
                                            : row.hasChildren
                                            ? const Icon(
                                                AleraIcons.chevronRight,
                                                size: 12,
                                                color:
                                                    AleraTokens.foregroundFaint,
                                              )
                                            : null,
                                      ),
                                      const SizedBox(width: AleraTokens.space4),
                                      Expanded(
                                        child: Tooltip(
                                          message: row.fullPath,
                                          child: Text(
                                            row.segment,
                                            maxLines: 1,
                                            overflow: .ellipsis,
                                            style: TextStyle(
                                              fontSize: 12,
                                              fontWeight: selected
                                                  ? .w700
                                                  : .w400,
                                            ),
                                          ),
                                        ),
                                      ),
                                      if (row.branch != null)
                                        AleraIconButton(
                                          key: ValueKey(
                                            'workspace-branch-favorite:${row.branch}',
                                          ),
                                          tooltip:
                                              favorites.contains(row.branch)
                                              ? context.tr(
                                                  'Remove From Favorites',
                                                )
                                              : context.tr('Add To Favorites'),
                                          icon: favorites.contains(row.branch)
                                              ? Icons.star_rounded
                                              : Icons.star_border_rounded,
                                          iconSize: 16,
                                          minSize: 24,
                                          iconColor:
                                              favorites.contains(row.branch)
                                              ? AleraTokens.warning
                                              : AleraTokens.foregroundFaint,
                                          onPressed: () => ref
                                              .read(
                                                workbenchControllerProvider
                                                    .notifier,
                                              )
                                              .toggleBranchFavorite(
                                                scopeKey: scopeKey,
                                                branch: row.branch!,
                                              ),
                                        ),
                                    ],
                                  ),
                                ),
                              ),
                            );
                          },
                        ),
                ),
              ],
            ),
          ),
        ),
      ],
      builder: (context, controller, child) => Material(
        color: Colors.transparent,
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          onSecondaryTapDown: (details) =>
              unawaited(_showCopyBranchMenu(details.globalPosition, label)),
          child: InkWell(
            key: const ValueKey<String>('workspace-branch-status'),
            onTap: _switching
                ? null
                : controller.isOpen
                ? controller.close
                : _openMenu,
            mouseCursor: WidgetStateMouseCursor.clickable,
            child: Tooltip(
              message: '${context.tr('Current Branch')}: $label',
              child: Container(
                height: AleraTokens.statusBarHeight,
                constraints: const BoxConstraints(maxWidth: 192),
                padding: const EdgeInsets.symmetric(
                  horizontal: AleraTokens.space6,
                ),
                decoration: const BoxDecoration(
                  border: Border(
                    left: BorderSide(color: AleraTokens.borderSubtle),
                  ),
                ),
                child: Row(
                  mainAxisSize: .min,
                  children: <Widget>[
                    const Icon(
                      AleraIcons.gitBranch,
                      size: 13,
                      color: AleraTokens.foregroundMuted,
                    ),
                    const SizedBox(width: AleraTokens.space4),
                    Flexible(
                      child: Text(
                        label,
                        maxLines: 1,
                        overflow: .ellipsis,
                        style: AleraTokens.monoStyle.copyWith(fontSize: 10),
                      ),
                    ),
                    const SizedBox(width: AleraTokens.space4),
                    if (_switching || _loading)
                      const SizedBox(
                        width: 10,
                        height: 10,
                        child: CircularProgressIndicator(strokeWidth: 1.5),
                      )
                    else
                      const Icon(
                        AleraIcons.chevronDown,
                        size: 12,
                        color: AleraTokens.foregroundMuted,
                      ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
