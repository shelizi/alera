import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_profiles/application/local_agent_providers.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_acknowledgements.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_launch_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('new Agent requires a filtered Project, Workspace, then Agent', (
    tester,
  ) async {
    final now = DateTime.utc(2026, 11, 27);
    Project project(String id, String name) => Project(
      id: id,
      name: name,
      repoPath: 'C:/fixture/$id',
      createdAt: now,
      updatedAt: now,
    );
    Workspace workspace(String id, String projectId, String name) => Workspace(
      id: id,
      projectId: projectId,
      name: name,
      path: 'C:/fixture/$id',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceKind.main,
      status: WorkspaceStatus.active,
    );
    final controller = _DashboardWorkbenchController(
      WorkbenchState(
        projects: [project('a', 'Alpha'), project('b', 'Beta')],
        workspacesByProject: {
          'a': [
            workspace('a-main', 'a', 'Alpha Main'),
            workspace('a-dev', 'a', 'Alpha Dev'),
          ],
          'b': [workspace('b-main', 'b', 'Beta Main')],
        },
      ),
    );
    final container = ProviderContainer(
      overrides: [
        workbenchControllerProvider.overrideWith(() => controller),
        installedAgentClisProvider.overrideWith(
          (ref) async => [
            AgentType.codex,
            AgentType.claude,
            AgentType.agy,
            AgentType.copilot,
            AgentType.opencode,
          ],
        ),
      ],
    );
    addTearDown(container.dispose);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: GlobalAgentLaunchDialog()),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final projectMenu = find.byKey(const ValueKey('agent-create-project'));
    Finder workspaceMenu(String? project) =>
        find.byKey(ValueKey(('agent-create-workspace', project)));
    Finder agentMenu(String? workspace) =>
        find.byKey(ValueKey(('agent-create-agent', workspace)));
    Finder field(Finder menu) =>
        find.descendant(of: menu, matching: find.byType(TextField));
    final create = find.widgetWithText(FilledButton, 'Create Agent');
    expect(tester.widget<FilledButton>(create).onPressed, isNull);
    expect(
      tester.widget<DropdownMenu<String>>(workspaceMenu(null)).enabled,
      isFalse,
    );
    expect(
      tester.widget<DropdownMenu<AgentType>>(agentMenu(null)).enabled,
      isFalse,
    );
    await tester.tap(field(projectMenu));
    await tester.enterText(field(projectMenu), 'Alp');
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(MenuItemButton, 'Alpha'));
    await tester.pumpAndSettle();
    final alpha = tester.widget<DropdownMenu<String>>(workspaceMenu('a'));
    expect(alpha.dropdownMenuEntries.map((entry) => entry.label), [
      'Alpha Main',
      'Alpha Dev',
    ]);
    await tester.tap(field(workspaceMenu('a')));
    await tester.enterText(field(workspaceMenu('a')), 'Dev');
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(MenuItemButton, 'Alpha Dev'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<DropdownMenu<AgentType>>(agentMenu('a-dev'))
          .dropdownMenuEntries
          .map((entry) => entry.value),
      [
        AgentType.codex,
        AgentType.claude,
        AgentType.agy,
        AgentType.copilot,
        AgentType.opencode,
      ],
    );
    await tester.tap(field(agentMenu('a-dev')));
    await tester.enterText(
      field(agentMenu('a-dev')),
      agentDisplayName(AgentType.agy),
    );
    await tester.pumpAndSettle();
    await tester.tap(
      find.widgetWithText(MenuItemButton, agentDisplayName(AgentType.agy)),
    );
    await tester.pumpAndSettle();
    expect(tester.widget<FilledButton>(create).onPressed, isNotNull);
    // Editing text alone never authorizes creating the previously selected Agent.
    await tester.tap(field(projectMenu));
    await tester.enterText(field(projectMenu), 'Bet');
    await tester.pumpAndSettle();
    expect(tester.widget<FilledButton>(create).onPressed, isNull);
    await tester.tap(find.widgetWithText(MenuItemButton, 'Beta'));
    await tester.pumpAndSettle();
    final beta = tester.widget<DropdownMenu<String>>(workspaceMenu('b'));
    expect(beta.dropdownMenuEntries.map((entry) => entry.label), ['Beta Main']);
    expect(tester.widget<FilledButton>(create).onPressed, isNull);
    expect(
      tester.widget<DropdownMenu<AgentType>>(agentMenu(null)).enabled,
      isFalse,
    );
    expect(tester.takeException(), isNull);
  });

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

    expect(find.text('Agents'), findsOneWidget);
    expect(find.byType(Dialog), findsNothing);
    expect(find.byTooltip('Close'), findsOneWidget);

    await tester.tap(find.byTooltip('Close'));
    await tester.pump();
    expect(container.read(globalAgentDashboardNavigationProvider), isFalse);
  });

  testWidgets(
    'live dashboard reorders keyed cards without rebuilding unrelated content',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1600, 1600));
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
      expect(find.text('Interrupt'), findsNothing);
      expect(find.text('Terminate'), findsNothing);
      expect(find.text('Original conversation'), findsNothing);
      expect(tester.widget<Text>(find.text('Task a')).maxLines, 6);
      Finder more(String id) => find.descendant(
        of: card(id),
        matching: find.byTooltip('More actions'),
      );
      await tester.tap(more('a'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Interrupt'));
      await tester.pumpAndSettle();
      expect(action('a', 'Confirm interrupt'), findsOneWidget);
      expect(action('b', 'Confirm interrupt'), findsNothing);
      expect(tester.widget(find.text('Task b')), same(originalB));

      // Re-selecting the menu action is not the explicit confirmation.
      await tester.tap(more('a'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Interrupt'));
      await tester.pumpAndSettle();
      expect(action('a', 'Confirm interrupt'), findsOneWidget);
      await tester.tap(
        find.descendant(
          of: card('a'),
          matching: find.byTooltip('Cancel confirmation'),
        ),
      );
      await tester.pumpAndSettle();
      expect(action('a', 'Confirm interrupt'), findsNothing);
      await tester.tap(more('a'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Hide (keeps running)'));
      await tester.pumpAndSettle();
      expect(card('a'), findsNothing);
      expect(find.byTooltip('2 of 3 Agent sessions'), findsOneWidget);

      // A hidden Agent cannot disappear from the Needs You queue.
      statuses.replace({
        ...container.read(agentStatusControllerProvider),
        'session-a': status('a', state: AgentStatusState.waiting),
      });
      await tester.pumpAndSettle();
      await tester.tap(find.text('Needs You (1)'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const PageStorageKey<String>('agent-column-waiting')),
        findsOneWidget,
      );
      expect(
        find.byKey(const PageStorageKey<String>('agent-column-working')),
        findsNothing,
      );
      expect(card('a'), findsOneWidget);
      expect(card('b'), findsNothing);

      await tester.tap(find.text('Needs You (1)'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Show hidden cards'));
      await tester.pumpAndSettle();
      await tester.tap(more('a'));
      await tester.pumpAndSettle();
      expect(find.text('Show card'), findsOneWidget);
      await tester.tapAt(const Offset(5, 5));
      await tester.pumpAndSettle();
      expect(
        tester.getTopLeft(card('a')).dx,
        lessThan(tester.getTopLeft(card('b')).dx),
      );
      expect(tester.getTopLeft(card('c')).dx, tester.getTopLeft(card('b')).dx);

      // Done uses completion time, not the time of later background reports.
      statuses.replace({
        'session-a': status(
          'a',
          state: AgentStatusState.done,
        ).copyWith(stateStartedAt: now.add(const Duration(seconds: 2))),
        'session-b': status(
          'b',
          state: AgentStatusState.done,
        ).copyWith(updatedAt: now.add(const Duration(seconds: 9))),
        'session-c': status(
          'c',
          state: AgentStatusState.done,
        ).copyWith(stateStartedAt: now.add(const Duration(seconds: 3))),
      });
      await tester.pumpAndSettle();
      expect(find.text('● Unread'), findsNWidgets(3));
      final receipts = workbenchTabCompletionAcknowledgementsControllerProvider;
      final sidebarStatus = container.read(
        agentStatusControllerProvider,
      )['session-a']!;
      // The existing sidebar/tab strip acknowledgement must immediately
      // update the board, without switching workspaces or opening the card.
      container.read(receipts.notifier).acknowledge(sidebarStatus);
      await tester.pumpAndSettle();
      expect(action('a', 'Read'), findsOneWidget);
      expect(
        tester.getTopLeft(card('b')).dy,
        lessThan(tester.getTopLeft(card('a')).dy),
      );
      container.read(receipts.notifier).markUnread(sidebarStatus);
      await tester.pumpAndSettle();
      expect(action('a', '● Unread'), findsOneWidget);
      expect(
        tester.getTopLeft(card('c')).dy,
        lessThan(tester.getTopLeft(card('a')).dy),
      );
      expect(
        tester.getTopLeft(card('a')).dy,
        lessThan(tester.getTopLeft(card('b')).dy),
      );
      await tester.tap(more('c'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mark as read'));
      await tester.pumpAndSettle();
      expect(action('c', 'Read'), findsOneWidget);
      expect(
        isCompletionAcknowledged(
          container.read(receipts),
          container.read(agentStatusControllerProvider)['session-c'],
        ),
        isTrue,
      );
      // Reverting from the board also changes the state the sidebar reads.
      await tester.tap(more('c'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mark as unread'));
      await tester.pumpAndSettle();
      expect(
        isCompletionAcknowledged(
          container.read(receipts),
          container.read(agentStatusControllerProvider)['session-c'],
        ),
        isFalse,
      );
      await tester.tap(more('c'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Mark as read'));
      await tester.pumpAndSettle();
      expect(
        tester.getTopLeft(card('b')).dy,
        lessThan(tester.getTopLeft(card('c')).dy),
      );
      final current = container.read(agentStatusControllerProvider);
      statuses.replace({
        ...current,
        'session-c': current['session-c']!.copyWith(
          updatedAt: now.add(const Duration(seconds: 20)),
        ),
      });
      await tester.pumpAndSettle();
      expect(action('c', 'Read'), findsOneWidget);
      statuses.replace({
        ...container.read(agentStatusControllerProvider),
        'session-c': current['session-c']!.copyWith(
          stateStartedAt: now.add(const Duration(seconds: 21)),
          updatedAt: now.add(const Duration(seconds: 21)),
        ),
      });
      await tester.pumpAndSettle();
      expect(action('c', '● Unread'), findsOneWidget);
      expect(
        tester.getTopLeft(card('c')).dy,
        lessThan(tester.getTopLeft(card('a')).dy),
      );

      // Search changes visibility without changing the cached inventory.
      await tester.enterText(find.byType(TextField).first, 'task b');
      await tester.pumpAndSettle();
      expect(card('b'), findsOneWidget);
      expect(card('a'), findsNothing);
      expect(card('c'), findsNothing);

      // Historical search should remain mounted through live status updates.
      await tester.tap(find.byTooltip('History Search'));
      await tester.pumpAndSettle();
      final originalHeader = tester.widget(find.text('Agents'));
      statuses.replace({});
      await tester.pumpAndSettle();
      expect(tester.widget(find.text('Agents')), same(originalHeader));
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
