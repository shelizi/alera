import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef AgentSessionPresentation = ({
  AgentType agentType,
  String? taskTitle,
  String stateLabel,
  String goal,
  String storedTitle,
  String overallSummary,
  String progress,
  bool isSummary,
});

String _bounded(String text, int limit) => String.fromCharCodes(
  text.replaceAll(RegExp(r'[\x00-\x1F\x7F]+'), ' ').trim().runes.take(limit),
);

/// One local, identity-checked presentation for sidebar and workbench tabs.
/// Read only the bounded Host overview, not tool input or intermediate output.
/// The opt-in controls inline task text. Deliberate hover shows the bounded
/// overview on both surfaces without any extra Host requests.
AgentSessionPresentation? agentSessionPresentation({
  required WorkspaceTabRecord tab,
  required AgentStatusEntry? status,
  required bool showTaskSummaries,
  required bool showTabTitle,
}) {
  if (status == null || !agentStatusMatchesTab(tab, status)) {
    return null;
  }
  final overview = AgentContextOverview.fromRaw(status.contextOverview);
  final goal = overview.goal.isNotEmpty
      ? overview.goal
      : showTaskSummaries
      ? _bounded(status.prompt, 600)
      : '';
  final storedTitle = tab.title.trim();
  final taskTitle = showTaskSummaries && goal.isNotEmpty
      ? goal
      : showTabTitle && storedTitle.isNotEmpty
      ? storedTitle
      : null;
  final stateLabel = status.interrupted == true
      ? 'Interrupted'
      : switch (status.state) {
          AgentStatusState.waiting => 'Waiting for input',
          AgentStatusState.blocked => 'Blocked',
          AgentStatusState.working => 'Working',
          AgentStatusState.done => 'Done',
        };
  final progress = overview.displayProgress;
  return (
    agentType: status.agentType,
    taskTitle: taskTitle,
    stateLabel: stateLabel,
    goal: goal,
    storedTitle: storedTitle,
    overallSummary: overview.overallSummary,
    progress: progress,
    isSummary: overview.reportSummary.isNotEmpty,
  );
}

String agentSessionTooltip(
  AgentSessionPresentation presentation,
  String Function(String) translate, {
  String? stateLabel,
}) => [
  '${agentDisplayName(presentation.agentType)} · ${stateLabel ?? translate(presentation.stateLabel)}',
  if (presentation.goal.isNotEmpty)
    '${translate('Overall Objective')}: ${presentation.goal}',
  if (presentation.goal.isEmpty && presentation.storedTitle.isNotEmpty)
    presentation.storedTitle,
  if (presentation.overallSummary.isNotEmpty)
    '${translate('Overall Progress Summary')}: ${presentation.overallSummary}',
  if (presentation.progress.isNotEmpty)
    '${translate(presentation.isSummary ? 'Latest Report · AI Summary' : 'Latest Report · Original Reply')}: ${presentation.progress}',
].join('\n\n');
