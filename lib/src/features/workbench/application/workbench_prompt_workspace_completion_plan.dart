final class WorkbenchPromptWorkspaceCompletionPlan {
  const WorkbenchPromptWorkspaceCompletionPlan({
    required this.ensureInitialTerminal,
    required this.agentTabIdToRefocus,
  });

  final bool ensureInitialTerminal;
  final String? agentTabIdToRefocus;
}

WorkbenchPromptWorkspaceCompletionPlan planWorkbenchPromptWorkspaceCompletion({
  required String? agentTabId,
  required String? deferredSetupCommand,
}) {
  final resolvedAgentTabId = _nonBlank(agentTabId);
  final setupCommand = _nonBlank(deferredSetupCommand);
  return WorkbenchPromptWorkspaceCompletionPlan(
    ensureInitialTerminal: resolvedAgentTabId == null && setupCommand == null,
    agentTabIdToRefocus: resolvedAgentTabId,
  );
}

String? _nonBlank(String? value) {
  final trimmed = value?.trim();
  return trimmed == null || trimmed.isEmpty ? null : trimmed;
}
