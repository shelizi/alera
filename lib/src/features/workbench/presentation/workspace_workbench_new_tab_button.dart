part of 'workspace_workbench_view.dart';

sealed class _NewTabMenuAction {
  const _NewTabMenuAction();
}

final class _NewTerminalMenuAction extends _NewTabMenuAction {
  const _NewTerminalMenuAction();
}

final class _NewAgentMenuAction extends _NewTabMenuAction {
  const _NewAgentMenuAction(this.agentType);

  final AgentType agentType;
}

class const _NewTabButton({
  required final String groupId,
  required final VoidCallback onCreateTab,
  required final ValueChanged<AgentType> onCreateAgentTab,
}) extends ConsumerWidget {
  Future<void> _openMenu(
    BuildContext context,
    List<AgentType> detectedAgents,
  ) async {
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
    final selected = await showMenu<_NewTabMenuAction>(
      context: context,
      position: .fromRect(
        .fromPoints(topLeft, bottomRight),
        Offset.zero & overlay.size,
      ),
      items: <PopupMenuEntry<_NewTabMenuAction>>[
        const AleraDropdownEntry<_NewTabMenuAction>(
          value: _NewTerminalMenuAction(),
          label: 'New Terminal',
          leading: Icon(
            AleraIcons.terminal,
            size: 16,
            color: AleraTokens.foregroundMuted,
          ),
        ),
        if (detectedAgents.isNotEmpty)
          const PopupMenuDivider(height: AleraTokens.space8),
        for (final agentType in detectedAgents)
          AleraDropdownEntry<_NewTabMenuAction>(
            value: _NewAgentMenuAction(agentType),
            label: agentDisplayName(agentType),
            localizeLabel: false,
            leading: AgentIdentityIcon(
              agentType: agentType,
              size: 16,
              showTooltip: false,
            ),
          ),
      ],
    );

    if (selected == null) {
      return;
    }

    switch (selected) {
      case _NewTerminalMenuAction():
        onCreateTab();
      case _NewAgentMenuAction(:final agentType):
        onCreateAgentTab(agentType);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final detectedAgents =
        ref.watch(installedAgentClisProvider).asData?.value ??
        const <AgentType>[];
    return AleraIconButton(
      tooltip: 'New Tab',
      icon: AleraIcons.add,
      iconSize: 16,
      minSize: 28,
      hoverColor: AleraTokens.surfaceElevated,
      borderRadius: AleraTokens.radiusSm,
      onPressed: () => unawaited(_openMenu(context, detectedAgents)),
    );
  }
}
