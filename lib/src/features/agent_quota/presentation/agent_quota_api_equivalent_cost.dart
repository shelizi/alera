import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_quota/application/agent_quota_api_equivalent_summary.dart';
import 'package:alera/src/features/agent_quota/domain/agent_quota.dart';
import 'package:alera/src/features/agent_usage/application/agent_usage_providers.dart'
    hide AgentUsageProvider;
import 'package:alera/src/features/agent_usage/domain/agent_usage.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Mounted only while a quota hover card is open. Reuses the existing
/// cached transcript scanner instead of running a second pricing algorithm.
class const AgentQuotaApiEquivalentCost({
  super.key,
  required final String hostId,
  required final AgentQuotaSnapshot snapshot,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final provider = switch (snapshot.provider) {
      AgentQuotaProviderId.claude => AgentUsageProvider.claude,
      AgentQuotaProviderId.codex => AgentUsageProvider.codex,
      _ => null,
    };
    if (provider == null) return const SizedBox.shrink();

    // The existing usage provider tracks the active workspace host. Never
    // present a different host's logs in the current quota account.
    final activeHostId = ref.watch(
      workbenchControllerProvider.select(
        (state) => state.activeWorkspace?.hostId ?? 'local',
      ),
    );
    if (activeHostId != hostId) return const SizedBox.shrink();

    // 31 days always covers the complete current calendar month plus 7 days.
    final usage = ref.watch(agentUsageProvider(31));
    final result = usage.value;
    final now = DateTime.now();
    final today = _dayLabel(now);
    final dateMatches = result?.snapshot.untilDay == today;
    final summary = dateMatches
        ? summarizeAgentQuotaApiEquivalentCost(
            usage: result!.snapshot,
            provider: provider,
            accountId: snapshot.accountId,
            now: now,
          )
        : null;
    final stateText = result == null || !dateMatches
        ? usage.hasError
              ? 'API cost history unavailable'
              : 'Reading local usage...'
        : summary == null
        ? 'No local usage history for this account'
        : null;
    final note = result?.refreshing == true
        ? 'Updating local history...'
        : summary?.hasUnpricedUsage == true || summary?.hasPartialSource == true
        ? 'Local usage incomplete (missing price or transcript)'
        : 'Local API-equivalent estimate, not billed';

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Text(
          'API-Equivalent Cost',
          style: Theme.of(context).textTheme.bodySmall
              ?.copyWith(color: AleraTokens.foregroundMuted, fontWeight: .w600),
        ),
        const SizedBox(height: AleraTokens.space6),
        if (stateText != null)
          Text(
            stateText,
            style: AleraTokens.monoStyle.copyWith(
              fontSize: 10,
              color: AleraTokens.foregroundFaint,
            ),
          )
        else ...<Widget>[
          _CostPeriodLine(label: 'Today', period: summary!.today),
          _CostPeriodLine(label: 'Last 7 days', period: summary.lastSevenDays),
          _CostPeriodLine(label: 'This month', period: summary.thisMonth),
          const SizedBox(height: AleraTokens.space4),
          Text(
            note,
            style: AleraTokens.monoStyle.copyWith(
              fontSize: 9,
              color: AleraTokens.foregroundFaint,
            ),
          ),
        ],
      ],
    );
  }
}

class const _CostPeriodLine({
  required final String label,
  required final AgentQuotaApiEquivalentPeriod period,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final value = period.costUsd;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AleraTokens.space2),
      child: Row(
        children: <Widget>[
          Expanded(
            child: Text(
              label,
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.foregroundMuted),
            ),
          ),
          Text(
            value == null ? 'Unavailable' : 'US\$${value.toStringAsFixed(2)}',
            style: AleraTokens.monoStyle.copyWith(
              fontSize: 11,
              color: AleraTokens.foreground,
              fontWeight: .w600,
            ),
          ),
        ],
      ),
    );
  }
}

String _dayLabel(DateTime day) =>
    '${day.year.toString().padLeft(4, '0')}-'
    '${day.month.toString().padLeft(2, '0')}-'
    '${day.day.toString().padLeft(2, '0')}';
