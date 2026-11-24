import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/agent_context_recovery.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_status_projection.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

// Only the fields actually displayed by the Context panel participate in
// its subscription. In particular, tool input/name, timestamps, and
// intermediate assistant output are not work reports.
typedef _AgentContextLiveReport = ({
  String provider,
  AgentContextOverview overview,
  String promptFallback,
  String completedReplyFallback,
});

_AgentContextLiveReport? _projectLiveReport(AgentStatusEntry? status) {
  if (status == null) return null;
  final overview = AgentContextOverview.fromRaw(status.contextOverview);
  return (
    provider: status.agentType.key,
    overview: overview,
    promptFallback: overview.goal.isEmpty ? status.prompt.trim() : '',
    completedReplyFallback:
        overview.displayProgress.isEmpty &&
            status.state == AgentStatusState.done &&
            status.interrupted != true
        ? (status.lastAssistantMessage?.trim() ?? '')
        : '',
  );
}

final _agentContextLiveReportProvider = Provider.autoDispose
    .family<_AgentContextLiveReport?, WorkspaceTabRecord>((ref, tab) {
      final status = ref.watch(
        agentStatusControllerProvider.select(
          (statuses) =>
              matchingAgentStatusForTab(tab: tab, agentStatuses: statuses),
        ),
      );
      return _projectLiveReport(status);
    });

/// Per-Agent overview. Selection is derived from the currently active tab,
/// not the most recently updated Agent in another workspace.
class const WorkspaceAgentContextPanel({
  super.key,
  required final Workspace workspace,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final tab = ref.watch(
      workbenchControllerProvider.select((state) {
        return state.activeWorkspace?.id == workspace.id
            ? state.activeWorkspaceTab
            : null;
      }),
    );
    final report = tab == null || tab.kind != WorkspaceTabKind.terminal
        ? null
        : ref.watch(_agentContextLiveReportProvider(tab));
    final provider = tab == null ? null : agentContextProviderForTab(tab);
    if (tab == null ||
        tab.kind != WorkspaceTabKind.terminal ||
        (report == null && provider == null)) {
      return const Center(
        child: Padding(
          padding: EdgeInsets.all(20),
          child: Text(
            '選取一個 Agent Tab，即可查看整體目標與工作時間線。',
            textAlign: TextAlign.center,
          ),
        ),
      );
    }

    final overview = report?.overview ?? const AgentContextOverview();
    final identity = (
      workspaceId: tab.workspaceId,
      tabId: tab.id,
      sessionId: tab.terminalSessionId,
      conversationId: tab.payload['agentTitleConversationId']?.toString() ?? '',
      provider: report?.provider ?? provider ?? '',
    );
    // Only the visible selected tab can request its exact verified history.
    // Live Hook reports take precedence. A separate checkpoint can survive a
    // Host restart or SessionEnd without making the Agent appear idle.
    final needsRecovery =
        overview.goal.isEmpty ||
        overview.displayProgress.isEmpty ||
        overview.timeline.isEmpty;
    // Invalidate the saved view when presence disappears, including a Host
    // restart and SessionEnd. A previously cached empty result must not hide
    // a more recent durable report. Selection changes use a different key.
    ref.listen(_agentContextLiveReportProvider(tab), (previous, next) {
      if (previous != null && next == null) {
        ref.invalidate(agentContextRecoveryProvider(identity));
      }
    });
    final recovered = needsRecovery
        ? ref.watch(agentContextRecoveryProvider(identity))
        : null;
    return AgentContextReportView._projected(
      key: ValueKey(identity),
      sessionId: tab.terminalSessionId,
      report: report,
      recovery: recovered?.asData?.value,
      loading: recovered?.isLoading ?? false,
      refresh: needsRecovery
          ? () => ref.invalidate(agentContextRecoveryProvider(identity))
          : null,
    );
  }
}

/// Actual right-hand panel body. Kept separate from selection/transport so
/// report visibility, empty states and recovery precedence can be exercised
/// by a real widget test without starting a Terminal.
class AgentContextReportView extends StatelessWidget {
  const AgentContextReportView({
    super.key,
    required this.sessionId,
    required this.live,
    this.recovery,
    this.loading = false,
    this.refresh,
  }) : _report = null;

  const AgentContextReportView._projected({
    super.key,
    required this.sessionId,
    required _AgentContextLiveReport? report,
    this.recovery,
    this.loading = false,
    this.refresh,
  }) : _report = report,
       live = null;

  final String sessionId;
  final AgentStatusEntry? live;
  final _AgentContextLiveReport? _report;
  final AgentContextRecovery? recovery;
  final bool loading;
  final VoidCallback? refresh;

  @override
  Widget build(BuildContext context) {
    final report = _report ?? _projectLiveReport(live);
    final overview = report?.overview ?? const AgentContextOverview();
    final stored = recovery?.overview ?? const AgentContextOverview();
    final prompt = report?.promptFallback ?? '';
    final goal = overview.goal.isNotEmpty
        ? overview.goal
        : prompt.isNotEmpty
        ? prompt
        : stored.goal;
    final raw = report?.completedReplyFallback ?? '';
    final usesStored = overview.displayProgress.isEmpty && raw.isEmpty;
    final progress = overview.displayProgress.isNotEmpty
        ? overview.displayProgress
        : raw.isNotEmpty
        ? raw
        : stored.progress;
    final textStyle = Theme.of(context).textTheme.bodySmall;
    // Keep recovered earlier milestones as well as the new live ones.
    final timeline = mergeAgentContextTimelines(
      overview.timeline,
      stored.timeline,
    );
    return ListView(
      key: ValueKey<String>('agent-context-$sessionId'),
      padding: const EdgeInsets.all(14),
      children: <Widget>[
        Row(
          children: <Widget>[
            const Icon(AleraIcons.ai, size: 16, color: AleraTokens.accent),
            const SizedBox(width: 8),
            Text(
              'Agent Context',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const Spacer(),
            if (refresh != null)
              IconButton(
                tooltip: '重新讀取工作回報',
                icon: const Icon(Icons.refresh, size: 16),
                onPressed: loading ? null : refresh,
              ),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          '整體目標會隨任務重點更新；工作回報取自 Agent 回覆或 AI 摘要，不代表已完成驗證。',
          style: textStyle?.copyWith(color: AleraTokens.foregroundMuted),
        ),
        const SizedBox(height: 18),
        _ContextSection(
          title: '整體作業目標',
          content: goal.isNotEmpty ? goal : '尚未取得任務目標。請先在 Agent 輸入工作需求。',
        ),
        const SizedBox(height: 14),
        _ContextSection(
          title: overview.reportSummary.isNotEmpty
              ? '最新工作回報（AI 摘要）'
              : usesStored && progress.isNotEmpty
              ? (recovery?.fromHistory == true
                    ? '最新工作回報（原生歷史回覆）'
                    : '最新工作回報（已保存的 Agent 回覆）')
              : '最新工作回報（Agent 回覆）',
          content: progress.isNotEmpty
              ? progress
              : loading
              ? '正在讀取原生工作回報…'
              : '尚未取得原始 Agent 回覆。完成事件可能不包含回覆內容；可重新讀取，或等待下一次工作回報。',
        ),
        const SizedBox(height: 18),
        Text('進度時間線（最新在前）', style: Theme.of(context).textTheme.titleSmall),
        const SizedBox(height: 8),
        if (timeline.isEmpty)
          Text(
            '尚無進度紀錄。',
            style: textStyle?.copyWith(color: AleraTokens.foregroundMuted),
          ),
        for (final event in timeline)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                const Icon(Icons.circle, size: 7, color: AleraTokens.accent),
                const SizedBox(width: 9),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: <Widget>[
                      Text(
                        _eventLabel(event.kind),
                        style: textStyle?.copyWith(fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(height: 3),
                      SelectableText(event.text, style: textStyle),
                      const SizedBox(height: 2),
                      Text(
                        _dateLabel(event.at),
                        style: textStyle?.copyWith(
                          color: AleraTokens.foregroundMuted,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class const _ContextSection({
  required final String title,
  required final String content,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(
      color: AleraTokens.surface,
      borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
      border: Border.all(color: AleraTokens.borderSubtle),
    ),
    child: Padding(
      padding: const EdgeInsets.all(12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Text(title, style: Theme.of(context).textTheme.labelLarge),
          const SizedBox(height: 7),
          SelectableText(content, style: Theme.of(context).textTheme.bodySmall),
        ],
      ),
    ),
  );
}

String _eventLabel(String kind) => switch (kind) {
  'goal' => '任務目標',
  'instruction' => '補充指示',
  'report' => 'Agent 工作回報',
  _ => '進度更新',
};

String _dateLabel(DateTime value) {
  final date = value.toLocal();
  String two(int x) => x.toString().padLeft(2, '0');
  return '${date.year}-${two(date.month)}-${two(date.day)} '
      '${two(date.hour)}:${two(date.minute)}';
}
