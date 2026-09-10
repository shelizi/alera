import 'package:alera/src/features/workbench/application/workbench_prompt_workspace_completion_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('selects without a blank terminal, opens setup, then refocuses the normalized agent tab', () async {
    final events = <String>[];
    final creation = _creation(deferredSetupCommand: 'pnpm install');
    final coordinator = WorkbenchPromptWorkspaceCompletionCoordinator(
      selectWorkspace: ({required ensureInitialTerminal}) async {
        events.add('select:$ensureInitialTerminal');
      },
      openDeferredSetupTab: (creation) async {
        events.add('setup:${creation.workspace.id}');
      },
      refocusAgentTab: (tabId) => events.add('refocus:$tabId'),
    );

    await coordinator.run(creation: creation, agentTabId: ' agent-tab ');

    expect(events, <String>[
      'select:false',
      'setup:workspace',
      'refocus:agent-tab',
    ]);
  });

  test(
    'blank prompt metadata seeds one initial terminal and does not refocus',
    () async {
      final events = <String>[];
      final creation = _creation(deferredSetupCommand: '   ');
      final coordinator = WorkbenchPromptWorkspaceCompletionCoordinator(
        selectWorkspace: ({required ensureInitialTerminal}) async {
          events.add('select:$ensureInitialTerminal');
        },
        openDeferredSetupTab: (_) async => events.add('setup'),
        refocusAgentTab: (tabId) => events.add('refocus:$tabId'),
      );

      await coordinator.run(creation: creation, agentTabId: '   ');

      expect(events, <String>['select:true', 'setup']);
    },
  );

  test('selection failure stops setup and refocus and propagates', () async {
    final events = <String>[];
    final coordinator = WorkbenchPromptWorkspaceCompletionCoordinator(
      selectWorkspace: ({required ensureInitialTerminal}) async {
        events.add('select');
        throw StateError('select failed');
      },
      openDeferredSetupTab: (_) async => events.add('setup'),
      refocusAgentTab: (tabId) => events.add('refocus'),
    );

    await expectLater(
      coordinator.run(
        creation: _creation(deferredSetupCommand: 'setup'),
        agentTabId: 'agent-tab',
      ),
      throwsA(isA<StateError>()),
    );
    expect(events, <String>['select']);
  });

  test('setup failure stops agent refocus and propagates', () async {
    final events = <String>[];
    final coordinator = WorkbenchPromptWorkspaceCompletionCoordinator(
      selectWorkspace: ({required ensureInitialTerminal}) async {
        events.add('select');
      },
      openDeferredSetupTab: (_) async {
        events.add('setup');
        throw StateError('setup failed');
      },
      refocusAgentTab: (tabId) => events.add('refocus'),
    );

    await expectLater(
      coordinator.run(
        creation: _creation(deferredSetupCommand: 'setup'),
        agentTabId: 'agent-tab',
      ),
      throwsA(isA<StateError>()),
    );
    expect(events, <String>['select', 'setup']);
  });
}

WorkspaceCreationResult _creation({String? deferredSetupCommand}) =>
    WorkspaceCreationResult(
      workspace: Workspace(
        id: 'workspace',
        projectId: 'project',
        name: 'Workspace',
        path: 'C:/repo/workspace',
        createdAt: DateTime.utc(2026, 9, 11),
        updatedAt: DateTime.utc(2026, 9, 11),
        kind: WorkspaceKind.linked,
        status: WorkspaceStatus.active,
      ),
      setupReport: .empty,
      deferredSetupCommand: deferredSetupCommand,
    );
