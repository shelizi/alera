import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/widgets/agent_session_presentation.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 11, 27);
  final tab = WorkspaceTabRecord(
    id: 'tab-1',
    workspaceId: 'workspace-1',
    title: 'Task title',
    createdAt: now,
    updatedAt: now,
  );
  final status = AgentStatusEntry(
    terminalSessionId: 'tab-1',
    workspaceId: 'workspace-1',
    tabId: 'tab-1',
    agentType: AgentType.codex,
    state: AgentStatusState.working,
    prompt: 'Task prompt',
    toolInput: 'do not reveal tool input',
    updatedAt: now,
    stateStartedAt: now,
    contextOverview: {
      'goal': 'Task objective',
      'progress': 'Original report',
      'reportSummary': 'AI report',
      'overallSummary': 'Cumulative report',
    },
  );
  AgentSessionPresentation? project(
    AgentStatusEntry? entry, {
    bool inline = true,
    bool title = false,
  }) => agentSessionPresentation(
    tab: tab,
    status: entry,
    showTaskSummaries: inline,
    showTabTitle: title,
  );

  test('shared title and tooltip prefer AI summary over original report', () {
    final result = project(status)!;
    expect(result.taskTitle, 'Task objective');
    expect(result.agentType, AgentType.codex);
    expect(result.progress, 'AI report');
    final tooltip = agentSessionTooltip(result, (label) => label);
    expect(tooltip, contains('Task objective'));
    expect(tooltip, contains('Cumulative report'));
    expect(tooltip, contains('Latest Report · AI Summary: AI report'));
    expect(tooltip, isNot(contains('Original report')));
    expect(tooltip, isNot(contains('do not reveal tool input')));
  });

  test('compact mode retains hover report and optional stored title', () {
    final result = project(status, inline: false, title: true)!;
    expect(result.taskTitle, 'Task title');
    expect(result.progress, 'AI report');
    expect(
      agentSessionTooltip(result, (label) => label),
      contains('AI report'),
    );
    expect(project(status, inline: false)!.taskTitle, isNull);
  });

  test('original report fallback is explicitly labeled', () {
    final entry = status.copyWith(
      contextOverview: {
        'goal': 'Task objective',
        'progress': 'Original report',
      },
    );
    expect(
      agentSessionTooltip(project(entry)!, (label) => label),
      contains('Latest Report · Original Reply: Original report'),
    );
  });

  test('foreign session or workspace cannot supply task presentation', () {
    expect(project(status.copyWith(workspaceId: 'foreign')), isNull);
    expect(project(status.copyWith(tabId: 'foreign')), isNull);
    expect(project(status.copyWith(terminalSessionId: 'foreign')), isNull);
    expect(project(null), isNull);
  });
}
