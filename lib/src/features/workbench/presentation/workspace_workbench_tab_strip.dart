part of 'workspace_workbench_view.dart';

class const _WorkspaceTabStrip({
  required final Workspace workspace,
  required final WorkspaceSourceControlScope? sourceControlScope,
  required final String groupId,
  required final List<WorkspaceTabRecord> tabs,
  required final String? activeTabId,
  required final bool canCloseSplit,
  required final TerminalRuntime terminalRuntime,
  required final Map<String, AgentStatusEntry> agentStatuses,
  required final ValueChanged<String> onSelectTab,
  required final ValueChanged<String> onCloseTab,
  required final ValueChanged<List<String>> onCloseTabs,
  required final RenameWorkspaceTabCallback onRenameTab,
  final OpenExternalTerminalCallback? onOpenExternalTerminal,
  required final VoidCallback onCreateTab,
  required final ValueChanged<AgentType> onCreateAgentTab,
  required final ValueChanged<WorkbenchDropZone> onSplitGroup,
  required final VoidCallback onMergeGroup,
  required final MoveWorkspaceTabCallback onMoveTab,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<_WorkspaceTabStrip> createState() => _WorkspaceTabStripState();
}

class _WorkspaceTabStripState extends ConsumerState<_WorkspaceTabStrip> {
  static const double _tabRevealMargin = AleraTokens.space12;

  final ScrollController _scrollController = ScrollController();
  final GlobalKey _scrollViewportKey = GlobalKey();
  final Map<String, GlobalKey> _tabKeys = <String, GlobalKey>{};
  bool _hasOverflow = false;
  bool _canScrollLeft = false;
  bool _canScrollRight = false;
  int? _insertionGapIndex;
  String? _lastSelectedPreviewTabId;
  DateTime? _lastSelectedPreviewAt;

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_syncOverflow);
    _syncCompletionAcknowledgement();
    _scheduleActiveTabReveal(animate: false);
  }

  @override
  void didUpdateWidget(covariant _WorkspaceTabStrip oldWidget) {
    super.didUpdateWidget(oldWidget);
    _syncCompletionAcknowledgement();
    _pruneTabKeys();
    if (oldWidget.activeTabId != widget.activeTabId ||
        _tabOrderChanged(oldWidget.tabs, widget.tabs)) {
      _scheduleActiveTabReveal();
    }
  }

  @override
  void dispose() {
    _scrollController.removeListener(_syncOverflow);
    _scrollController.dispose();
    super.dispose();
  }

  void _syncOverflow() {
    if (!mounted || !_scrollController.hasClients) {
      return;
    }
    final position = _scrollController.position;
    final overflow = position.maxScrollExtent > 0.5;
    final canScrollLeft = overflow && position.pixels > 0.5;
    final canScrollRight =
        overflow && position.pixels < position.maxScrollExtent - 0.5;
    final overflowChanged = overflow != _hasOverflow;
    if (overflowChanged ||
        canScrollLeft != _canScrollLeft ||
        canScrollRight != _canScrollRight) {
      setState(() {
        _hasOverflow = overflow;
        _canScrollLeft = canScrollLeft;
        _canScrollRight = canScrollRight;
      });
      if (overflowChanged) {
        _scheduleActiveTabReveal(animate: false);
      }
    }
  }

  bool _tabOrderChanged(
    List<WorkspaceTabRecord> previous,
    List<WorkspaceTabRecord> current,
  ) {
    if (previous.length != current.length) {
      return true;
    }
    for (var index = 0; index < previous.length; index += 1) {
      if (previous[index].id != current[index].id) {
        return true;
      }
    }
    return false;
  }

  GlobalKey _tabKey(String tabId) {
    return _tabKeys.putIfAbsent(tabId, GlobalKey.new);
  }

  void _pruneTabKeys() {
    final tabIds = <String>{for (final tab in widget.tabs) tab.id};
    _tabKeys.removeWhere((tabId, _) => !tabIds.contains(tabId));
  }

  void _scheduleActiveTabReveal({bool animate = true}) {
    final activeTabId = widget.activeTabId;
    if (activeTabId == null) {
      return;
    }
    _scheduleTabReveal(activeTabId, animate: animate);
  }

  void _scheduleTabReveal(String tabId, {bool animate = true}) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _ensureTabVisible(tabId, animate: animate);
      }
    });
  }

  void _ensureTabVisible(String tabId, {required bool animate}) {
    if (!_scrollController.hasClients) {
      return;
    }
    final tabContext = _tabKeys[tabId]?.currentContext;
    final viewportContext = _scrollViewportKey.currentContext;
    final tabBox = tabContext?.findRenderObject();
    final viewportBox = viewportContext?.findRenderObject();
    if (tabBox is! RenderBox ||
        viewportBox is! RenderBox ||
        !tabBox.hasSize ||
        !viewportBox.hasSize) {
      return;
    }

    final tabTopLeft = tabBox.localToGlobal(Offset.zero);
    final viewportTopLeft = viewportBox.localToGlobal(Offset.zero);
    final tabRect = tabTopLeft & tabBox.size;
    final viewportRect = viewportTopLeft & viewportBox.size;
    final leftEdge = viewportRect.left + _tabRevealMargin;
    final rightEdge = viewportRect.right - _tabRevealMargin;

    double delta = 0;
    if (tabRect.left < leftEdge) {
      delta = tabRect.left - leftEdge;
    } else if (tabRect.right > rightEdge) {
      delta = tabRect.right - rightEdge;
    }
    if (delta.abs() <= 0.5) {
      return;
    }
    final target = _scrollController.position.pixels + delta;
    if (animate) {
      _animateScrollTo(target);
    } else {
      _jumpScrollTo(target);
    }
  }

  void _jumpScrollTo(double target) {
    if (!_scrollController.hasClients) {
      return;
    }
    final position = _scrollController.position;
    final clamped = target.clamp(0.0, position.maxScrollExtent).toDouble();
    if ((clamped - position.pixels).abs() > 0.5) {
      _scrollController.jumpTo(clamped);
    }
  }

  void _animateScrollTo(double target) {
    if (!_scrollController.hasClients) {
      return;
    }
    final position = _scrollController.position;
    final clamped = target.clamp(0.0, position.maxScrollExtent).toDouble();
    if ((clamped - position.pixels).abs() <= 0.5) {
      return;
    }
    unawaited(
      _scrollController.animateTo(
        clamped,
        duration: const Duration(milliseconds: 140),
        curve: Curves.easeOutCubic,
      ),
    );
  }

  void _scrollTabs(int direction) {
    if (!_scrollController.hasClients) {
      return;
    }
    final position = _scrollController.position;
    final page = math.max(
      120.0,
      math.min(position.viewportDimension * 0.65, 260.0),
    );
    _animateScrollTo(position.pixels + (page * direction));
  }

  void _handlePointerSignal(PointerSignalEvent event) {
    if (event is! PointerScrollEvent ||
        !_hasOverflow ||
        !_scrollController.hasClients) {
      return;
    }
    final delta = event.scrollDelta;
    // Horizontal trackpads already drive a horizontal ScrollView natively.
    // Translate a normal vertical mouse wheel into horizontal tab-strip motion.
    if (delta.dy == 0 || delta.dy.abs() < delta.dx.abs()) {
      return;
    }
    final position = _scrollController.position;
    final target = (position.pixels + delta.dy)
        .clamp(0.0, position.maxScrollExtent)
        .toDouble();
    if ((target - position.pixels).abs() > 0.5) {
      _scrollController.jumpTo(target);
    }
  }

  void _syncCompletionAcknowledgement() {
    final activeTabId = widget.activeTabId;
    if (activeTabId == null) {
      return;
    }
    WorkspaceTabRecord? activeTab;
    for (final tab in widget.tabs) {
      if (tab.id == activeTabId) {
        activeTab = tab;
        break;
      }
    }
    if (activeTab == null) {
      return;
    }
    final sessionId = activeTab.terminalSessionId;
    // Provider mutations are not allowed inside widget life-cycles, so the
    // acknowledgement lands after this frame instead of directly here.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        return;
      }
      ref
          .read(
            workbenchTabCompletionAcknowledgementsControllerProvider.notifier,
          )
          .acknowledge(widget.agentStatuses[sessionId]);
    });
  }

  int? _resolvedDropIndex(_WorkspaceTabDragData data, int gapIndex) {
    return resolveWorkbenchTabStripDropIndex(
      tabIds: <String>[for (final tab in widget.tabs) tab.id],
      sourceGroupId: data.sourceGroupId,
      targetGroupId: widget.groupId,
      draggedTabId: data.tabId,
      gapIndex: gapIndex,
    );
  }

  void _handleGapHover(_WorkspaceTabDragData data, int gapIndex) {
    final next = _resolvedDropIndex(data, gapIndex) == null ? null : gapIndex;
    if (next != _insertionGapIndex) {
      setState(() => _insertionGapIndex = next);
    }
  }

  void _handleGapLeave() {
    if (_insertionGapIndex != null) {
      setState(() => _insertionGapIndex = null);
    }
  }

  void _maybeKeepPreviewTab(WorkspaceTabRecord tab) {
    final onKeep = _KeepPreviewTabScope.maybeOf(context);
    if (onKeep == null || !tab.isPreview) {
      _lastSelectedPreviewTabId = null;
      _lastSelectedPreviewAt = null;
      return;
    }
    final now = DateTime.now();
    if (_lastSelectedPreviewTabId == tab.id &&
        _lastSelectedPreviewAt != null &&
        now.difference(_lastSelectedPreviewAt!) <= kDoubleTapTimeout) {
      onKeep(tab.id);
      _lastSelectedPreviewTabId = null;
      _lastSelectedPreviewAt = null;
      return;
    }
    _lastSelectedPreviewTabId = tab.id;
    _lastSelectedPreviewAt = now;
  }

  void _handleGapDrop(_WorkspaceTabDragData data, int gapIndex) {
    setState(() => _insertionGapIndex = null);
    final index = _resolvedDropIndex(data, gapIndex);
    if (index == null) {
      return;
    }
    unawaited(
      widget.onMoveTab(
        tabId: data.tabId,
        targetGroupId: widget.groupId,
        zone: .center,
        index: index,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) => _syncOverflow());
    final acknowledgedCompletions = ref.watch(
      workbenchTabCompletionAcknowledgementsControllerProvider,
    );
    final addButton = _NewTabButton(
      groupId: widget.groupId,
      onCreateTab: widget.onCreateTab,
      onCreateAgentTab: widget.onCreateAgentTab,
    );
    return ColoredBox(
      color: AleraTokens.surface,
      child: _TabStripAppendDropTarget(
        workspaceId: widget.workspace.id,
        tabCount: widget.tabs.length,
        onHoverGap: _handleGapHover,
        onLeave: _handleGapLeave,
        onDropGap: _handleGapDrop,
        child: SizedBox(
          height: AleraTokens.sidebarHeaderHeight,
          child: Row(
            children: <Widget>[
              Expanded(
                child: Listener(
                  key: _scrollViewportKey,
                  behavior: HitTestBehavior.opaque,
                  onPointerSignal: _handlePointerSignal,
                  child: SingleChildScrollView(
                    key: ValueKey<String>(
                      'workspace-tab-scroll:${widget.groupId}',
                    ),
                    controller: _scrollController,
                    scrollDirection: .horizontal,
                    padding: const EdgeInsets.symmetric(
                      horizontal: AleraTokens.space8,
                      vertical: AleraTokens.space6,
                    ),
                    child: Row(
                      mainAxisSize: .min,
                      children: <Widget>[
                        for (final (index, tab) in widget.tabs.indexed)
                          SizedBox(
                            key: _tabKey(tab.id),
                            child: _TabStripChipDropTarget(
                              chipIndex: index,
                              workspaceId: widget.workspace.id,
                              showLeadingIndicator:
                                  index == 0 && _insertionGapIndex == 0,
                              showTrailingIndicator:
                                  _insertionGapIndex == index + 1,
                              onHoverGap: _handleGapHover,
                              onLeave: _handleGapLeave,
                              onDropGap: _handleGapDrop,
                              child: _DraggableWorkspaceTabChip(
                                workspace: widget.workspace,
                                sourceControlScope: widget.sourceControlScope,
                                groupId: widget.groupId,
                                tab: tab,
                                active: tab.id == widget.activeTabId,
                                terminalRuntime: widget.terminalRuntime,
                                status:
                                    widget.agentStatuses[tab.terminalSessionId],
                                completionAcknowledged:
                                    isCompletionAcknowledged(
                                      acknowledgedCompletions,
                                      widget.agentStatuses[tab
                                          .terminalSessionId],
                                    ),
                                groupTabs: widget.tabs,
                                onSelect: () {
                                  ref
                                      .read(
                                        workbenchTabCompletionAcknowledgementsControllerProvider
                                            .notifier,
                                      )
                                      .acknowledge(
                                        widget.agentStatuses[tab
                                            .terminalSessionId],
                                      );
                                  widget.onSelectTab(tab.id);
                                  _scheduleTabReveal(tab.id);
                                  _maybeKeepPreviewTab(tab);
                                },
                                onClose: () => widget.onCloseTab(tab.id),
                                onCloseTabs: widget.onCloseTabs,
                                onRename: (title) => widget.onRenameTab(
                                  tabId: tab.id,
                                  title: title,
                                ),
                                onOpenExternalTerminal:
                                    widget.onOpenExternalTerminal,
                                onSplit: widget.onSplitGroup,
                              ),
                            ),
                          ),
                        if (!_hasOverflow) addButton,
                      ],
                    ),
                  ),
                ),
              ),
              if (_hasOverflow) ...[
                AleraIconButton(
                  key: ValueKey<String>(
                    'workspace-tab-scroll-left:${widget.groupId}',
                  ),
                  tooltip: 'Scroll tabs left',
                  onPressed: _canScrollLeft ? () => _scrollTabs(-1) : null,
                  icon: AleraIcons.chevronLeft,
                  iconSize: 14,
                  minSize: 24,
                  iconColor: _canScrollLeft
                      ? AleraTokens.foregroundMuted
                      : AleraTokens.foregroundFaint,
                  hoverColor: AleraTokens.surfaceElevated,
                  borderRadius: AleraTokens.radiusSm,
                ),
                AleraIconButton(
                  key: ValueKey<String>(
                    'workspace-tab-scroll-right:${widget.groupId}',
                  ),
                  tooltip: 'Scroll tabs right',
                  onPressed: _canScrollRight ? () => _scrollTabs(1) : null,
                  icon: AleraIcons.chevronRight,
                  iconSize: 14,
                  minSize: 24,
                  iconColor: _canScrollRight
                      ? AleraTokens.foregroundMuted
                      : AleraTokens.foregroundFaint,
                  hoverColor: AleraTokens.surfaceElevated,
                  borderRadius: AleraTokens.radiusSm,
                ),
              ],
              if (_hasOverflow)
                Padding(
                  padding: const EdgeInsets.only(right: AleraTokens.space8),
                  child: addButton,
                ),
              _PaneMenuButton(
                canCloseSplit: widget.canCloseSplit,
                onSplitGroup: widget.onSplitGroup,
                onMergeGroup: widget.onMergeGroup,
              ),
              const SizedBox(width: AleraTokens.space4),
            ],
          ),
        ),
      ),
    );
  }
}

class const _PaneMenuButton({
  required final bool canCloseSplit,
  required final ValueChanged<WorkbenchDropZone> onSplitGroup,
  required final VoidCallback onMergeGroup,
}) extends StatelessWidget {
  Future<void> _openMenu(BuildContext context) async {
    final button = context.findRenderObject()! as RenderBox;
    final overlay =
        Navigator.of(context).overlay!.context.findRenderObject()! as RenderBox;
    final topLeft = button.localToGlobal(
      button.size.bottomLeft(.zero),
      ancestor: overlay,
    );
    final bottomRight = button.localToGlobal(
      button.size.bottomRight(.zero),
      ancestor: overlay,
    );
    final selected = await showMenu<_PaneMenuAction>(
      context: context,
      position: .fromRect(
        .fromPoints(topLeft, bottomRight),
        Offset.zero & overlay.size,
      ),
      items: <PopupMenuEntry<_PaneMenuAction>>[
        const AleraDropdownEntry<_PaneMenuAction>(
          value: .splitRight,
          label: 'Split Right',
          leading: _SplitDirectionGlyph(zone: .right),
        ),
        const AleraDropdownEntry<_PaneMenuAction>(
          value: .splitDown,
          label: 'Split Down',
          leading: _SplitDirectionGlyph(zone: .down),
        ),
        const AleraDropdownEntry<_PaneMenuAction>(
          value: .splitLeft,
          label: 'Split Left',
          leading: _SplitDirectionGlyph(zone: .left),
        ),
        const AleraDropdownEntry<_PaneMenuAction>(
          value: .splitUp,
          label: 'Split Up',
          leading: _SplitDirectionGlyph(zone: .up),
        ),
        if (canCloseSplit) const PopupMenuDivider(height: AleraTokens.space8),
        if (canCloseSplit)
          const AleraDropdownEntry<_PaneMenuAction>(
            value: .closeSplit,
            label: 'Close Split',
          ),
      ],
    );

    if (selected == null) {
      return;
    }

    switch (selected) {
      case _PaneMenuAction.splitRight:
        onSplitGroup(.right);
      case _PaneMenuAction.splitDown:
        onSplitGroup(.down);
      case _PaneMenuAction.splitLeft:
        onSplitGroup(.left);
      case _PaneMenuAction.splitUp:
        onSplitGroup(.up);
      case _PaneMenuAction.closeSplit:
        onMergeGroup();
    }
  }

  @override
  Widget build(BuildContext context) {
    return AleraIconButton(
      tooltip: 'Pane actions',
      onPressed: () => unawaited(_openMenu(context)),
      icon: AleraIcons.more,
      minSize: 28,
    );
  }
}

class const _SplitDirectionGlyph({required final WorkbenchDropZone zone})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      size: const .square(14),
      painter: _SplitDirectionPainter(zone: zone),
    );
  }
}

class const _SplitDirectionPainter({required final WorkbenchDropZone zone})
    extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final outerRect = Rect.fromLTWH(0.5, 0.5, size.width - 1, size.height - 1);
    final outerRRect = RRect.fromRectAndRadius(
      outerRect,
      const .circular(AleraTokens.radiusSm),
    );

    final fillRect = splitDirectionFillRectForTesting(zone, size);

    if (!fillRect.isEmpty) {
      canvas
        ..save()
        ..clipRRect(outerRRect)
        ..drawRect(fillRect, Paint()..color = AleraTokens.foreground)
        ..restore();
    }

    canvas.drawRRect(
      outerRRect,
      Paint()
        ..color = AleraTokens.foregroundMuted
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1,
    );
  }

  @override
  bool shouldRepaint(covariant _SplitDirectionPainter oldDelegate) {
    return splitDirectionShouldRepaintForTesting(oldDelegate.zone, zone);
  }
}
