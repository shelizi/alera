import 'package:alera/src/features/workbench/application/workbench_prompt_workspace_completion_plan.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('agent tab suppresses blank terminal and is normalized for refocus', () {
    final plan = planWorkbenchPromptWorkspaceCompletion(
      agentTabId: '  agent-tab  ',
      deferredSetupCommand: null,
    );

    expect(plan.ensureInitialTerminal, isFalse);
    expect(plan.agentTabIdToRefocus, 'agent-tab');
  });

  test('deferred setup suppresses blank terminal even without an agent id', () {
    final plan = planWorkbenchPromptWorkspaceCompletion(
      agentTabId: null,
      deferredSetupCommand: '  pnpm install  ',
    );

    expect(plan.ensureInitialTerminal, isFalse);
    expect(plan.agentTabIdToRefocus, isNull);
  });

  test('blank prompt metadata falls back to one initial terminal', () {
    final plan = planWorkbenchPromptWorkspaceCompletion(
      agentTabId: '   ',
      deferredSetupCommand: '  ',
    );

    expect(plan.ensureInitialTerminal, isTrue);
    expect(plan.agentTabIdToRefocus, isNull);
  });
}
