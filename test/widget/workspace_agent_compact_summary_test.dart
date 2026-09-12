import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_run_groups.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/widgets/workspace_agent_compact_summary.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('shows each group glyph with its run count', (tester) async {
    await _pumpSummary(
      tester,
      groupWorkspaceAgentRuns(<WorkspaceAgentRun>[
        _run(.claude, tabId: 'tab-1', state: .working),
        _run(.cursor, tabId: 'tab-2', state: .working),
        _run(.claude, tabId: 'tab-3', state: .done),
      ]),
    );

    expect(find.text('2'), findsOneWidget);
    expect(find.text('1'), findsOneWidget);
  });

  testWidgets('unacked completions render with the warning tint', (
    tester,
  ) async {
    await _pumpSummary(
      tester,
      groupWorkspaceAgentRuns(<WorkspaceAgentRun>[
        _run(.claude, tabId: 'tab-1', state: .done),
      ]),
    );

    final icon = tester.widget<Icon>(find.byIcon(AleraIcons.success));
    expect(icon.color, AleraTokens.warning);
  });

  testWidgets('acknowledged completions render with the done tint', (
    tester,
  ) async {
    final run = _run(.claude, tabId: 'tab-1', state: .done);
    await _pumpSummary(
      tester,
      groupWorkspaceAgentRuns(
        <WorkspaceAgentRun>[run],
        acknowledgedCompletions: <String, DateTime>{
          run.status.terminalSessionId: run.status.stateStartedAt,
        },
      ),
    );

    final icon = tester.widget<Icon>(find.byIcon(AleraIcons.success));
    expect(icon.color, AleraTokens.success);
  });

  testWidgets('collapses groups beyond the visible limit into a +N count', (
    tester,
  ) async {
    await _pumpSummary(
      tester,
      groupWorkspaceAgentRuns(<WorkspaceAgentRun>[
        _run(.claude, tabId: 'tab-1', state: .waiting),
        _run(.claude, tabId: 'tab-2', state: .blocked),
        _run(.claude, tabId: 'tab-3', state: .done, interrupted: true),
        _run(.claude, tabId: 'tab-4', state: .working),
        _run(.cursor, tabId: 'tab-5', state: .working),
      ]),
    );

    // waiting, blocked and interrupted stay visible; the two working runs
    // collapse into the trailing overflow count.
    expect(find.text('+2'), findsOneWidget);
  });
}

Future<void> _pumpSummary(
  WidgetTester tester,
  List<WorkspaceAgentRunGroup> groups,
) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: WorkspaceAgentCompactSummary(
          groups: groups,
          expanded: false,
          onToggle: () {},
        ),
      ),
    ),
  );
  await tester.pump();
}

WorkspaceAgentRun _run(
  AgentType agentType, {
  required String tabId,
  AgentStatusState state = .working,
  bool? interrupted,
}) {
  final now = DateTime.utc(2026, 5, 26, 12);
  return WorkspaceAgentRun(
    tab: WorkspaceTabRecord(
      id: tabId,
      workspaceId: 'workspace-1',
      title: 'Terminal',
      createdAt: now,
      updatedAt: now,
    ),
    status: AgentStatusEntry(
      terminalSessionId: 'session-$tabId',
      workspaceId: 'workspace-1',
      tabId: tabId,
      agentType: agentType,
      state: state,
      prompt: 'Run tests',
      updatedAt: now,
      stateStartedAt: now,
      interrupted: interrupted,
    ),
  );
}
