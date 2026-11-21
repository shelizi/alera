part of 'project_workbench_sidebar.dart';

class const _AgentRunRow({
  super.key,
  required final WorkspaceTabRecord tab,
  required final AgentStatusEntry status,
  required final WorkspaceAgentGroupKind displayKind,
  required final bool isActive,
  required final VoidCallback onTap,
  required final VoidCallback onClose,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<_AgentRunRow> createState() => _AgentRunRowState();
}

class _AgentRunRowState extends ConsumerState<_AgentRunRow> {
  bool _hovered = false;
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final isActive = widget.isActive;
    final actionsVisible = _hovered || isActive;
    ref.watch(agentTitleAvailableProvider);
    final showTabTitle = ref.watch(
      settingsControllerProvider.select(
        (settings) => settings.agents.showTabTitlesInSidebar,
      ),
    );
    final showTaskSummaries = ref.watch(
      settingsControllerProvider.select(
        (settings) => settings.agents.showTaskSummariesInSidebar,
      ),
    );
    final groupLabel = context.tr(workspaceAgentGroupLabel(widget.displayKind));
    // Opt-in only: don't parse or allocate summary history for hidden rows.
    final overview = showTaskSummaries
        ? AgentContextOverview.fromRaw(widget.status.contextOverview)
        : const AgentContextOverview();
    final description = _agentRunLabel(
      showTaskSummaries: showTaskSummaries,
      overview: overview,
      tab: widget.tab,
      status: widget.status,
      displayKind: widget.displayKind,
      showTabTitle: showTabTitle,
      groupLabel: groupLabel,
    );
    return MouseRegion(
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        onSecondaryTapDown: (details) =>
            unawaited(_openContextMenu(context, details.globalPosition)),
        child: AnimatedContainer(
          width: .infinity,
          duration: AleraTokens.durationFast,
          decoration: BoxDecoration(
            color: isActive
                ? AleraTokens.accentSubtle
                : (_hovered ? AleraTokens.surface : Colors.transparent),
            borderRadius: BorderRadius.circular(AleraTokens.radiusSm),
          ),
          child: InkWell(
            onTap: widget.onTap,
            mouseCursor: SystemMouseCursors.click,
            borderRadius: .circular(AleraTokens.radiusSm),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space6,
                vertical: AleraTokens.space4,
              ),
              child: Row(
                crossAxisAlignment: .center,
                children: <Widget>[
                  Tooltip(
                    message: groupLabel,
                    child: WorkspaceAgentGroupGlyph(
                      kind: widget.displayKind,
                      size: 12,
                    ),
                  ),
                  const SizedBox(width: AleraTokens.space6),
                  AgentIdentityIcon(
                    agentType: widget.status.agentType,
                    size: 13,
                    color: isActive
                        ? AleraTokens.foreground
                        : AleraTokens.foregroundMuted,
                  ),
                  const SizedBox(width: AleraTokens.space6),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: <Widget>[
                        Tooltip(
                          message: description,
                          child: Text(
                            description,
                            maxLines: showTaskSummaries
                                ? (_expanded ? null : 4)
                                : 1,
                            overflow: showTaskSummaries && _expanded
                                ? null
                                : TextOverflow.ellipsis,
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: isActive
                                  ? AleraTokens.foreground
                                  : AleraTokens.foregroundMuted,
                              fontWeight: showTaskSummaries || isActive
                                  ? FontWeight.w600
                                  : FontWeight.w500,
                            ),
                          ),
                        ),
                        if (showTaskSummaries &&
                            overview.progress.isNotEmpty) ...<Widget>[
                          const SizedBox(height: 3),
                          Text(
                            '最新回報：${overview.progress}',
                            maxLines: _expanded ? null : 2,
                            overflow: _expanded ? null : TextOverflow.ellipsis,
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: AleraTokens.foregroundMuted,
                            ),
                          ),
                        ],
                        if (showTaskSummaries &&
                            (description.length > 75 ||
                                overview.progress.length > 55))
                          InkWell(
                            onTap: () => setState(() => _expanded = !_expanded),
                            child: Padding(
                              padding: const EdgeInsets.only(top: 3),
                              child: Text(
                                _expanded ? '收合' : '展開全文',
                                style: theme.textTheme.labelSmall?.copyWith(
                                  color: AleraTokens.accent,
                                ),
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                  IgnorePointer(
                    ignoring: !actionsVisible,
                    child: AnimatedOpacity(
                      opacity: actionsVisible ? 1 : 0,
                      duration: AleraTokens.durationFast,
                      child: Padding(
                        padding: const EdgeInsets.only(
                          left: AleraTokens.space4,
                        ),
                        child: AleraIconButton(
                          tooltip: 'Close Terminal',
                          onPressed: widget.onClose,
                          icon: AleraIcons.close,
                          iconSize: 12,
                          minSize: 20,
                          borderRadius: AleraTokens.radiusSm,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _openContextMenu(
    BuildContext context,
    Offset globalPosition,
  ) async {
    if (ref.read(agentTitleAvailableProvider).value != true) {
      return;
    }
    final overlay =
        Navigator.of(context).overlay!.context.findRenderObject()! as RenderBox;
    final selected = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(
        Rect.fromPoints(globalPosition, globalPosition),
        Offset.zero & overlay.size,
      ),
      items: <PopupMenuEntry<String>>[
        AleraDropdownEntry<String>(
          value: 'generateTitle',
          label: agentTitleActionLabel(widget.tab.payload),
          enabled: !isAgentTitleGenerating(widget.tab.payload),
          leading: const Icon(AleraIcons.ai, size: 16),
        ),
      ],
    );
    if (selected != 'generateTitle' || !context.mounted) {
      return;
    }
    try {
      await ref.read(agentTitleServiceProvider).generate(widget.tab);
    } on Object catch (error) {
      AleraToast.publish(
        message: 'Could not generate title: $error',
        tone: .error,
      );
    }
  }
}

String _agentRunLabel({
  required bool showTaskSummaries,
  required AgentContextOverview overview,
  required WorkspaceTabRecord tab,
  required AgentStatusEntry status,
  required WorkspaceAgentGroupKind displayKind,
  required bool showTabTitle,
  required String groupLabel,
}) {
  if (showTaskSummaries) {
    if (overview.goal.isNotEmpty) return overview.goal;
    final prompt = status.prompt.trim();
    if (prompt.isNotEmpty) return prompt;
  }
  if (showTabTitle) {
    final title = tab.title.trim();
    if (title.isNotEmpty) {
      return title;
    }
  }
  return _agentRunDescription(
    status,
    displayKind: displayKind,
    groupLabel: groupLabel,
  );
}

String _agentRunDescription(
  AgentStatusEntry status, {
  required WorkspaceAgentGroupKind displayKind,
  String? groupLabel,
}) {
  // The concise, pre-Agent-Context row is the default presentation.
  if (displayKind == WorkspaceAgentGroupKind.working) {
    final toolName = status.toolName?.trim() ?? '';
    final toolInput = status.toolInput?.trim() ?? '';
    if (toolName.isNotEmpty && toolInput.isNotEmpty) {
      return '$toolName: $toolInput';
    }
    if (toolName.isNotEmpty) {
      return toolName;
    }
  }
  final assistantMessage = status.lastAssistantMessage?.trim() ?? '';
  if (assistantMessage.isNotEmpty) {
    return assistantMessage;
  }
  return '${agentDisplayName(status.agentType)} · ${groupLabel ?? workspaceAgentGroupLabel(displayKind)}';
}
