import 'package:alera/src/features/workbench/application/workbench_prompt_workspace_completion_plan.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';

typedef WorkbenchPromptWorkspaceSelector = Future<void> Function({
  required bool ensureInitialTerminal,
});
typedef WorkbenchPromptDeferredSetupOpener = Future<void> Function(
  WorkspaceCreationResult creation,
);
typedef WorkbenchPromptAgentRefocuser = void Function(String tabId);

final class WorkbenchPromptWorkspaceCompletionCoordinator {
  const WorkbenchPromptWorkspaceCompletionCoordinator({
    required WorkbenchPromptWorkspaceSelector selectWorkspace,
    required WorkbenchPromptDeferredSetupOpener openDeferredSetupTab,
    required WorkbenchPromptAgentRefocuser refocusAgentTab,
  }) : _selectWorkspace = selectWorkspace,
       _openDeferredSetupTab = openDeferredSetupTab,
       _refocusAgentTab = refocusAgentTab;

  final WorkbenchPromptWorkspaceSelector _selectWorkspace;
  final WorkbenchPromptDeferredSetupOpener _openDeferredSetupTab;
  final WorkbenchPromptAgentRefocuser _refocusAgentTab;

  Future<void> run({
    required WorkspaceCreationResult creation,
    String? agentTabId,
  }) async {
    final plan = planWorkbenchPromptWorkspaceCompletion(
      agentTabId: agentTabId,
      deferredSetupCommand: creation.deferredSetupCommand,
    );
    await _selectWorkspace(ensureInitialTerminal: plan.ensureInitialTerminal);
    await _openDeferredSetupTab(creation);
    final tabId = plan.agentTabIdToRefocus;
    if (tabId != null) {
      _refocusAgentTab(tabId);
    }
  }
}
