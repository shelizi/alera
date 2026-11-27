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
    // The checkpoint also owns manually generated summaries. Keep it observed
    // even when all live report fields are populated, so invalidating it after
    // generation actually refreshes the visible report.
    final needsRecovery =
        overview.goal.isEmpty ||
        (overview.progress.isEmpty && overview.reportSummary.isEmpty) ||
        overview.timeline.isEmpty;
    // Invalidate the saved view when presence disappears, including a Host
    // restart and SessionEnd. A previously cached empty result must not hide
    // a more recent durable report. Selection changes use a different key.
    ref.listen(_agentContextLiveReportProvider(tab), (previous, next) {
      if (previous != null && previous.overview != next?.overview) {
        ref.invalidate(agentContextRecoveryProvider(identity));
      }
    });
    final recovered = needsRecovery || identity.conversationId.isNotEmpty
        ? ref.watch(agentContextRecoveryProvider(identity))
        : null;
    return AgentContextReportView._projected(
      key: ValueKey(identity),
      sessionId: tab.terminalSessionId,
      report: report,
      recovery: recovered?.asData?.value,
      loading: recovered?.isLoading ?? false,
      refresh: recovered != null
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
              bool stillSelected() {
                if (!context.mounted) return false;
                final active = ref
                    .read(workbenchControllerProvider)
                    .activeWorkspaceTab;
                return active?.workspaceId == identity.workspaceId &&
                    active?.id == identity.tabId &&
                    active?.terminalSessionId == identity.sessionId &&
                    active?.payload['agentTitleConversationId']?.toString() ==
                        identity.conversationId;
              }

              String summaryFrom(AgentContextOverview value) {
                if (reportAt == null) return value.overallSummary;
                for (final event in value.timeline) {
                  if (event.kind == 'report' && event.reportAt == reportAt) {
                    return event.summary;
                  }
                }
                return '';
              }

              final storedBefore = summaryFrom(
                recovered?.asData?.value.overview ??
                    const AgentContextOverview(),
              );
              final before = storedBefore.isNotEmpty
                  ? storedBefore
                  : summaryFrom(overview);
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
              final requestId = result['requestId'];
              // Poll only after an explicit action. Completion means the
              // checkpoint has been updated, not merely that the job vanished.
              for (var i = 0; i < 90; i++) {
                if (!stillSelected()) return;
                await Future<void>.delayed(const Duration(seconds: 2));
                if (!stillSelected()) return;
                final status = await host.runtimeRequest(
                  'agentContext.get',
                  owner,
                );
                if (!stillSelected()) return;
                if (status is! Map || status['summaryJobs'] is! List) {
                  throw StateError('Host 未提供摘要狀態，請確認 Runtime Host 已更新。');
                }
                final results = status['summaryResults'];
                if (requestId is String && results is List) {
                  for (final outcome in results) {
                    if (outcome is! Map || outcome['requestId'] != requestId) {
                      continue;
                    }
                    if (outcome['status'] != 'succeeded') {
                      throw StateError(
                        outcome['message']?.toString() ?? '摘要生成失敗',
                      );
                    }
                    ref.invalidate(agentContextRecoveryProvider(identity));
                    await ref.read(
                      agentContextRecoveryProvider(identity).future,
                    );
                    return;
                  }
                }
                final pending = (status['summaryJobs'] as List).any(
                  (job) =>
                      job is Map &&
                      (requestId is String && job['requestId'] != null
                          ? job['requestId'] == requestId
                          : reportAt == null
                          ? job['overall'] == true
                          : job['overall'] != true &&
                                job['reportAt'] == reportAt),
                );
                if (pending) continue;
                final saved = AgentContextOverview.fromRaw(
                  status['contextOverview'],
                );
                final after = summaryFrom(saved);
                ref.invalidate(agentContextRecoveryProvider(identity));
                await ref.read(agentContextRecoveryProvider(identity).future);
                if (after.isNotEmpty && after != before) return;
                throw StateError(
                  '未取得新的摘要或完成狀態。請確認 AI Assist 模型、登入及 Runtime Host 版本，再重試。',
                );
              }
              throw StateError('摘要尚未完成，請稍後重新讀取工作回報。');
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
    // Use the saved summary only for an exact matching report. A checkpoint
    // from an earlier round must never replace a newer live reply.
    final timeline = mergeAgentContextTimelines(
      overview.timeline,
      stored.timeline,
      preferRecoveredSummaries: true,
    );
    final reports = timeline
        .where((event) => event.kind == 'report')
        .toList(growable: false);
    final liveReports = overview.timeline.where(
      (event) => event.kind == 'report',
    );
    final latestLive = liveReports.isEmpty ? null : liveReports.first;
    final latestReport = liveHasReport
        ? (latestLive == null
              ? null
              : reports
                    .where(
                      (event) =>
                          event.reportAt == latestLive.reportAt &&
                          event.text == latestLive.text,
                    )
                    .firstOrNull)
        : reports.firstOrNull;
    final savedSummary = latestReport?.summary ?? '';
    final summary = savedSummary.isNotEmpty
        ? savedSummary
        : liveHasReport
        ? overview.reportSummary
        : stored.reportSummary;
    final rawReply = liveHasReport
        ? (overview.progress.isNotEmpty ? overview.progress : fallback)
        : stored.progress;
    final displayedReport = summary.isNotEmpty ? summary : rawReply;
    final hasRawReport = summary.isEmpty && rawReply.isNotEmpty;
    final textStyle = Theme.of(context).textTheme.bodySmall;
    final overall = stored.overallSummary.isNotEmpty
        ? stored.overallSummary
        : overview.overallSummary;
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
          content: goal.isNotEmpty ? goal : '尚未取得任務目標。請先在 Agent 輸入工作需求。',
          expandable: goal.isNotEmpty,
          expansionLabel: '目標',
        ),
        if (overall.isNotEmpty ||
            (summarize != null && reports.isNotEmpty)) ...[
          const SizedBox(height: 14),
          _ContextSection(
            title: '整體進度摘要',
            content: overall.isNotEmpty ? overall : '尚未建立整體進度摘要。可整理已保留的工作回報。',
            expandable: overall.isNotEmpty,
            expansionLabel: '總摘要',
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
          action: summarize == null || latestReport == null
              ? null
              : _SummaryAction(
                  key: ValueKey((
                    'latest-summary-action',
                    latestReport.reportAt,
                  )),
                  label: summary.isEmpty ? '生成摘要' : '重做摘要',
                  onGenerate: () => summarize!(latestReport.reportAt),
                ),
        ),
        if (summary.isEmpty ||
            timeline.any(
              (event) => event.kind == 'report' && event.summary.isEmpty,
            )) ...[
          const SizedBox(height: 8),
          Text(
            '手動生成使用設定 → AI Assist → Agent Work Reports 的模型及 Instructions（自訂摘要指示），不必開啟自動摘要。可啟用 Auto-Summarize Agent Work Reports 自動整理後續回報。',
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
  final String? expansionLabel,
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
              expansionLabel: expansionLabel,
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
  String? _failure;
  @override
  Widget build(BuildContext context) => Tooltip(
    message: _failure ?? widget.label,
    child: TextButton.icon(
      style: TextButton.styleFrom(
        padding: const EdgeInsets.symmetric(horizontal: 4),
        minimumSize: const Size(32, 28),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
      onPressed: _busy
          ? null
          : () async {
              setState(() {
                _busy = true;
                _failure = null;
              });
              try {
                await widget.onGenerate();
                if (mounted) {
                  ScaffoldMessenger.maybeOf(context)
                      ?.showSnackBar(const SnackBar(content: Text('摘要已更新')));
                }
              } catch (error) {
                if (mounted) {
                  setState(() => _failure = error.toString());
                  ScaffoldMessenger.maybeOf(context)
                      ?.showSnackBar(SnackBar(content: Text('無法生成摘要：$error')));
                }
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
      label: Text(
        _busy
            ? '摘要中…'
            : _failure != null
            ? '摘要失敗，重試'
            : widget.label,
      ),
    ),
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
    this.expansionLabel,
  });
  final String content;
  final bool isSummary;
  final String? expansionLabel;
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
            if (_expanded && overflow)
              SelectableText(widget.content, style: style)
            else
              Text(
                widget.content,
                style: style,
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
              ),
            // Read the preview first, then reveal more using a compact,
            // directional control immediately below the content.
            if (overflow)
              TextButton(
                style: TextButton.styleFrom(
                  padding: EdgeInsets.zero,
                  minimumSize: const Size(48, 32),
                  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                ),
                onPressed: () => setState(() => _expanded = !_expanded),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      '${_expanded ? '收合' : '展開'}${widget.expansionLabel ?? (widget.isSummary ? '摘要' : '回覆')}',
                    ),
                    const SizedBox(width: 2),
                    Icon(
                      _expanded ? Icons.expand_less : Icons.expand_more,
                      size: 16,
                    ),
                  ],
                ),
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
