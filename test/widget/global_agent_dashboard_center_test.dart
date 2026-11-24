import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Global Agent Dashboard is a center page, not a modal dialog', (
    tester,
  ) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    container.read(globalAgentDashboardNavigationProvider.notifier).open();

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: GlobalAgentDashboardPage()),
        ),
      ),
    );

    expect(find.text('Global Agent Dashboard'), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    expect(find.byTooltip('Close'), findsOneWidget);

    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(container.read(globalAgentDashboardNavigationProvider), isFalse);
  });

  testWidgets(
    'live dashboard reorders keyed cards without rebuilding unrelated content',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1600, 1100));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final now = DateTime.utc(2026, 11, 24);
      final project = Project(
        id: 'project',
        name: 'Project',
        repoPath: 'C:/fixture',
        createdAt: now,
        updatedAt: now,
      );
      final workspace = Workspace(
        id: 'workspace',
        projectId: project.id,
        name: 'Workspace',
        path: 'C:/fixture',
        createdAt: now,
        updatedAt: now,
        kind: WorkspaceKind.main,
        status: WorkspaceStatus.active,
      );
      WorkspaceTabRecord tab(String id) => WorkspaceTabRecord(
        id: id,
        workspaceId: workspace.id,
        title: 'Agent $id',
        createdAt: now,
        updatedAt: now,
        payload: {workspaceTabTerminalSessionIdPayloadKey: 'session-$id'},
      );
      AgentStatusEntry status(
        String id, {
        AgentStatusState state = AgentStatusState.working,
      }) => AgentStatusEntry(
        terminalSessionId: 'session-$id',
        workspaceId: workspace.id,
        tabId: id,
        agentType: AgentType.codex,
        state: state,
        prompt: 'Task $id',
        updatedAt: now,
        stateStartedAt: now,
      );
      final workbench = _DashboardWorkbenchController(
        WorkbenchState(
          projects: [project],
          workspacesByProject: {
            project.id: [workspace],
          },
          tabsByWorkspace: {
            workspace.id: [tab('a'), tab('b'), tab('c')],
          },
        ),
      );
      final statuses = _DashboardStatusController({
        'session-a': status('a'),
        'session-b': status('b'),
        'session-c': status('c'),
      });
      final container = ProviderContainer(
        overrides: [
          workbenchControllerProvider.overrideWith(() => workbench),
          agentStatusControllerProvider.overrideWith(() => statuses),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const MaterialApp(
            home: Scaffold(body: GlobalAgentDashboardPage()),
          ),
        ),
      );
      await tester.pumpAndSettle();
      Finder card(String id) => find.byKey(
        ValueKey<Object>((
          sessionId: 'session-$id',
          workspaceId: 'workspace',
          tabId: id,
        )),
      );
      Finder action(String id, String label) =>
          find.descendant(of: card(id), matching: find.text(label));

      final originalA = tester.widget(find.text('Task a'));
      final originalB = tester.widget(find.text('Task b'));
      final originalAElement = tester.element(card('a'));
      final originalBElement = tester.element(card('b'));
      expect(
        tester.getTopLeft(card('a')).dy,
        lessThan(tester.getTopLeft(card('c')).dy),
      );

      statuses.replace({
        ...container.read(agentStatusControllerProvider),
        'session-c': status('c').copyWith(
          updatedAt: now.add(const Duration(seconds: 1)),
          prompt: 'Task c updated',
        ),
      });
      await tester.pumpAndSettle();
      expect(
        tester.getTopLeft(card('c')).dy,
        lessThan(tester.getTopLeft(card('a')).dy),
      );
      expect(find.text('Task c updated'), findsOneWidget);
      expect(tester.widget(find.text('Task a')), same(originalA));
      expect(tester.widget(find.text('Task b')), same(originalB));
      expect(tester.element(card('a')), same(originalAElement));
      expect(tester.element(card('b')), same(originalBElement));

      // Local actions must still update the intended card. First press only
      // arms the verified interrupt; never invoke the actual Host action.
      await tester.tap(action('a', 'Interrupt'));
      await tester.pumpAndSettle();
      expect(action('a', 'Confirm interrupt'), findsOneWidget);
      expect(action('b', 'Confirm interrupt'), findsNothing);
      expect(tester.widget(find.text('Task b')), same(originalB));

      await tester.tap(action('a', 'Hide (keeps running)'));
      await tester.pumpAndSettle();
      expect(card('a'), findsNothing);
      expect(find.text('2 of 3 Agent sessions'), findsOneWidget);

      // A hidden Agent cannot disappear from the Needs You queue.
      statuses.replace({
        ...container.read(agentStatusControllerProvider),
        'session-a': status('a', state: AgentStatusState.waiting),
      });
      await tester.pumpAndSettle();
      await tester.tap(find.text('Needs You (1)'));
      await tester.pumpAndSettle();
      expect(card('a'), findsOneWidget);
      expect(card('b'), findsNothing);

      await tester.tap(find.text('Show all Agents'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Show hidden cards (1)'));
      await tester.pumpAndSettle();
      expect(action('a', 'Show card'), findsOneWidget);

      // Search changes visibility without changing the cached inventory.
      await tester.enterText(find.byType(TextField).first, 'task b');
      await tester.pumpAndSettle();
      expect(card('b'), findsOneWidget);
      expect(card('a'), findsNothing);
      expect(card('c'), findsNothing);

      // Historical search should remain mounted through live status updates.
      await tester.tap(find.text('History Search'));
      await tester.pumpAndSettle();
      final originalHeader = tester.widget(find.text('Global Agent Dashboard'));
      statuses.replace({});
      await tester.pumpAndSettle();
      expect(
        tester.widget(find.text('Global Agent Dashboard')),
        same(originalHeader),
      );
      expect(tester.takeException(), isNull);
    },
  );
}

class _DashboardWorkbenchController extends WorkbenchController {
  _DashboardWorkbenchController(this.initial);
  final WorkbenchState initial;
  @override
  WorkbenchState build() => initial;
}

class _DashboardStatusController extends AgentStatusController {
  _DashboardStatusController(this.initial);
  final Map<String, AgentStatusEntry> initial;
  @override
  Map<String, AgentStatusEntry> build() => initial;
  void replace(Map<String, AgentStatusEntry> entries) => state = entries;
}
