import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
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
        overview.progress.isEmpty &&
            overview.reportSummary.isEmpty &&
            status.state == AgentStatusState.done &&
            status.interrupted != true
        ? String.fromCharCodes(
            (status.lastAssistantMessage ?? '').trim().runes.take(420),
          )
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
        (overview.progress.isEmpty && overview.reportSummary.isEmpty) ||
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
      summarize: identity.conversationId.isEmpty
          ? null
          : (reportAt) async {
              final host = ref.read(runtimeHostClientProvider);
              final owner = <String, Object?>{
                'workspaceId': identity.workspaceId,
                'tabId': identity.tabId,
                'sessionId': identity.sessionId,
                'conversationId': identity.conversationId,
              };
              try {
                final result = await host.runtimeRequest(
                  'agentContext.summarize',
                  {
                    ...owner,
                    if (reportAt == null)
                      'overall': true
                    else
                      'reportAt': reportAt,
                  },
                );
                if (result is! Map || result['queued'] != true) {
                  throw StateError('Host did not accept the summary request');
                }
                // Poll only after an explicit action, not during normal rendering.
                for (var i = 0; i < 90 && context.mounted; i++) {
                  await Future<void>.delayed(const Duration(seconds: 2));
                  if (!context.mounted) return;
                  final status = await host.runtimeRequest(
                    'agentContext.get',
                    owner,
                  );
                  if (status is! Map || status['summaryJobs'] is! List) break;
                  final pending = (status['summaryJobs'] as List).any(
                    (job) =>
                        job is Map &&
                        (reportAt == null
                            ? job['overall'] == true
                            : job['overall'] != true &&
                                  job['reportAt'] == reportAt),
                  );
                  if (!pending) break;
                }
                if (context.mounted)
                  ref.invalidate(agentContextRecoveryProvider(identity));
              } catch (error) {
                if (context.mounted) {
                  ScaffoldMessenger.maybeOf(context)
                      ?.showSnackBar(SnackBar(content: Text('無法生成摘要：$error')));
                }
              }
            },
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
    this.summarize,
  }) : _report = null;

  const AgentContextReportView._projected({
    super.key,
    required this.sessionId,
    required _AgentContextLiveReport? report,
    this.recovery,
    this.loading = false,
    this.refresh,
    this.summarize,
  }) : _report = report,
       live = null;

  final String sessionId;
  final AgentStatusEntry? live;
  final _AgentContextLiveReport? _report;
  final AgentContextRecovery? recovery;
  final bool loading;
  final VoidCallback? refresh;
  final Future<void> Function(String? reportAt)? summarize;

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
    final fallback = report?.completedReplyFallback ?? '';
    final liveHasReport =
        overview.progress.isNotEmpty ||
        overview.reportSummary.isNotEmpty ||
        fallback.isNotEmpty;
    final summary = liveHasReport
        ? overview.reportSummary
        : stored.reportSummary;
    final rawReply = liveHasReport
        ? (overview.progress.isNotEmpty ? overview.progress : fallback)
        : stored.progress;
    final displayedReport = summary.isNotEmpty ? summary : rawReply;
    final hasRawReport = summary.isEmpty && rawReply.isNotEmpty;
    final textStyle = Theme.of(context).textTheme.bodySmall;
    // Keep recovered earlier milestones as well as the new live ones.
    final timeline = mergeAgentContextTimelines(
      overview.timeline,
      stored.timeline,
    );
    final overall = overview.overallSummary.isNotEmpty
        ? overview.overallSummary
        : stored.overallSummary;
    final reports = timeline
        .where((event) => event.kind == 'report')
        .toList(growable: false);
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
          '工作回報優先顯示 AI 摘要；尚無摘要時顯示原始回覆。回報不代表已完成驗證。',
          style: textStyle?.copyWith(color: AleraTokens.foregroundMuted),
        ),
        const SizedBox(height: 18),
        _ContextSection(
          title: '整體作業目標',
          content: goal.isNotEmpty
              ? compactAgentContextTask(goal)
              : '尚未取得任務目標。請先在 Agent 輸入工作需求。',
        ),
        if (overall.isNotEmpty ||
            (summarize != null && reports.isNotEmpty)) ...[
          const SizedBox(height: 14),
          _ContextSection(
            title: '整體進度摘要',
            content: overall.isNotEmpty ? overall : '尚未建立整體進度摘要。可整理已保留的工作回報。',
            expandable: overall.isNotEmpty,
            action: summarize == null || reports.isEmpty
                ? null
                : _SummaryAction(
                    key: const ValueKey('overall-summary-action'),
                    label: overall.isEmpty ? '生成總摘要' : '重做總摘要',
                    onGenerate: () => summarize!(null),
                  ),
          ),
        ],
        const SizedBox(height: 14),
        _ContextSection(
          title: summary.isNotEmpty
              ? '最新工作回報（AI 摘要）'
              : hasRawReport
              ? '最新工作回報（原始回覆）'
              : '最新工作回報',
          content: displayedReport.isNotEmpty
              ? displayedReport
              : loading
              ? '正在讀取工作回報…'
              : '尚未取得工作回報。',
          expandable: displayedReport.isNotEmpty,
          isSummary: summary.isNotEmpty,
          action: summarize == null || reports.isEmpty
              ? null
              : _SummaryAction(
                  key: ValueKey((
                    'latest-summary-action',
                    reports.first.reportAt,
                  )),
                  label: summary.isEmpty ? '生成摘要' : '重做摘要',
                  onGenerate: () => summarize!(reports.first.reportAt),
                ),
        ),
        if (summary.isEmpty ||
            timeline.any(
              (event) => event.kind == 'report' && event.summary.isEmpty,
            )) ...[
          const SizedBox(height: 8),
          Text(
            '可至設定 → AI Assist → Agent Work Report 啟用 Auto-Summarize Agent Work Reports。摘要生成後會優先顯示。',
            style: textStyle?.copyWith(color: AleraTokens.foregroundMuted),
          ),
        ],
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
                        event.kind == 'report'
                            ? (event.summary.isNotEmpty
                                  ? 'Agent 工作回報（AI 摘要）'
                                  : 'Agent 工作回報（原始回覆）')
                            : _eventLabel(event.kind),
                        style: textStyle?.copyWith(fontWeight: FontWeight.w600),
                      ),
                      if (event.kind == 'report' && summarize != null)
                        _SummaryAction(
                          key: ValueKey((
                            'history-summary-action',
                            event.reportAt,
                          )),
                          label: event.summary.isEmpty ? '生成摘要' : '重做摘要',
                          onGenerate: () => summarize!(event.reportAt),
                        ),
                      const SizedBox(height: 3),
                      if (event.kind == 'report')
                        _ExpandableContextSummary(
                          key: ValueKey((
                            sessionId,
                            event.at,
                            event.text,
                            event.summary,
                          )),
                          content: event.summary.isNotEmpty
                              ? event.summary
                              : event.text,
                          isSummary: event.summary.isNotEmpty,
                        )
                      else
                        Text(
                          compactAgentContextTask(event.text),
                          style: textStyle,
                        ),
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
  final bool expandable = false,
  final bool isSummary = true,
  final Widget? action,
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
          if (action == null)
            Text(title, style: Theme.of(context).textTheme.labelLarge)
          else
            Row(
              children: [
                Expanded(
                  child: Text(
                    title,
                    style: Theme.of(context).textTheme.labelLarge,
                  ),
                ),
                action!,
              ],
            ),
          const SizedBox(height: 7),
          if (expandable)
            _ExpandableContextSummary(
              key: ValueKey((content, isSummary)),
              content: content,
              isSummary: isSummary,
            )
          else
            Text(content, style: Theme.of(context).textTheme.bodySmall),
        ],
      ),
    ),
  );
}

class _SummaryAction extends StatefulWidget {
  const _SummaryAction({
    super.key,
    required this.label,
    required this.onGenerate,
  });
  final String label;
  final Future<void> Function() onGenerate;
  @override
  State<_SummaryAction> createState() => _SummaryActionState();
}

class _SummaryActionState extends State<_SummaryAction> {
  bool _busy = false;
  @override
  Widget build(BuildContext context) => TextButton.icon(
    style: TextButton.styleFrom(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      minimumSize: const Size(32, 28),
      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
    ),
    onPressed: _busy
        ? null
        : () async {
            setState(() => _busy = true);
            try {
              await widget.onGenerate();
            } finally {
              if (mounted) setState(() => _busy = false);
            }
          },
    icon: _busy
        ? const SizedBox(
            width: 12,
            height: 12,
            child: CircularProgressIndicator(strokeWidth: 2),
          )
        : const Icon(Icons.refresh, size: 14),
    label: Text(_busy ? '摘要中…' : widget.label),
  );
}

/// Keep the task anchor short; never expand into an original prompt.
String compactAgentContextTask(String value) {
  final chars = value.trim().runes;
  return chars.length <= 120
      ? value.trim()
      : '${String.fromCharCodes(chars.take(117))}…';
}

class _ExpandableContextSummary extends StatefulWidget {
  const _ExpandableContextSummary({
    super.key,
    required this.content,
    this.isSummary = true,
  });
  final String content;
  final bool isSummary;
  @override
  State<_ExpandableContextSummary> createState() =>
      _ExpandableContextSummaryState();
}

class _ExpandableContextSummaryState extends State<_ExpandableContextSummary> {
  bool _expanded = false;
  @override
  Widget build(BuildContext context) {
    final style = Theme.of(context).textTheme.bodySmall;
    return LayoutBuilder(
      builder: (context, constraints) {
        final painter = TextPainter(
          text: TextSpan(text: widget.content, style: style),
          textDirection: Directionality.of(context),
          textScaler: MediaQuery.textScalerOf(context),
          maxLines: 3,
          ellipsis: '…',
        )..layout(maxWidth: constraints.maxWidth);
        final overflow = painter.didExceedMaxLines;
        painter.dispose();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Keep the toggle above a long expanded summary, so the user
            // does not need to scroll through the entire report to collapse.
            if (overflow)
              TextButton(
                style: TextButton.styleFrom(
                  padding: EdgeInsets.zero,
                  minimumSize: const Size(48, 32),
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                ),
                onPressed: () => setState(() => _expanded = !_expanded),
                child: Text(
                  widget.isSummary
                      ? (_expanded ? '收合摘要' : '展開摘要')
                      : (_expanded ? '收合回覆' : '展開回覆'),
                ),
              ),
            if (_expanded && overflow)
              SelectableText(widget.content, style: style)
            else
              Text(
                widget.content,
                style: style,
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
              ),
          ],
        );
      },
    );
  }
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
