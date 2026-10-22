import 'package:alera/src/features/agent_usage/domain/agent_usage.dart';

/// API-equivalent value of locally recorded subscription usage. It is never
/// the amount charged to an account, and may be incomplete if model prices
/// or transcript files are unavailable.
class const AgentQuotaApiEquivalentPeriod({
  required final double? costUsd,
  required final int records,
  required final int unpricedRecords,
}) {
  bool get isPartial => unpricedRecords > 0;
}

class const AgentQuotaApiEquivalentSummary({
  required final AgentQuotaApiEquivalentPeriod today,
  required final AgentQuotaApiEquivalentPeriod lastSevenDays,
  required final AgentQuotaApiEquivalentPeriod thisMonth,
  final bool hasPartialSource = false,
}) {
  bool get hasUnpricedUsage =>
      today.isPartial || lastSevenDays.isPartial || thisMonth.isPartial;
}

/// Computes three calendar-aligned periods from one existing 31-day scan.
///
/// Match both provider and account: a work Claude profile must never include
/// the default account, Codex, or another host's session totals.
AgentQuotaApiEquivalentSummary? summarizeAgentQuotaApiEquivalentCost({
  required AgentUsageSnapshot usage,
  required AgentUsageProvider provider,
  required String accountId,
  required DateTime now,
}) {
  final matching = <AgentUsageBucket>[
    for (final bucket in usage.buckets)
      if (bucket.provider == provider && bucket.accountId == accountId) bucket,
  ];
  final readableSource = usage.sources.any(
    (source) =>
        source.provider == provider &&
        source.accountId == accountId &&
        (source.status == AgentUsageSourceStatus.ok ||
            source.status == AgentUsageSourceStatus.partial),
  );
  if (!readableSource && matching.isEmpty) {
    return null;
  }

  // UTC calendar arithmetic avoids daylight-saving shifts in the date keys.
  final today = DateTime.utc(now.year, now.month, now.day);
  final todayKey = _usageDay(today);
  final weekKey = _usageDay(today.subtract(const Duration(days: 6)));
  final monthKey = _usageDay(DateTime(today.year, today.month, 1));

  final daily = _CostTotals();
  final weekly = _CostTotals();
  final monthly = _CostTotals();
  for (final bucket in matching) {
    // Buckets are local YYYY-MM-DD dates, so lexicographic ordering is safe.
    if (bucket.day.compareTo(todayKey) > 0) continue;
    if (bucket.day == todayKey) daily.add(bucket);
    if (bucket.day.compareTo(weekKey) >= 0) weekly.add(bucket);
    if (bucket.day.compareTo(monthKey) >= 0) monthly.add(bucket);
  }
  return AgentQuotaApiEquivalentSummary(
    today: daily.freeze(),
    lastSevenDays: weekly.freeze(),
    thisMonth: monthly.freeze(),
    hasPartialSource: usage.sources.any(
      (source) =>
          source.provider == provider &&
          source.accountId == accountId &&
          source.status == AgentUsageSourceStatus.partial,
    ),
  );
}

class _CostTotals {
  double costUsd = 0.0;
  int records = 0;
  int unpricedRecords = 0;

  void add(AgentUsageBucket bucket) {
    costUsd += bucket.costUsd;
    records += bucket.records;
    unpricedRecords += bucket.unpricedRecords;
  }

  AgentQuotaApiEquivalentPeriod freeze() => AgentQuotaApiEquivalentPeriod(
    // Unknown pricing must not masquerade as US$0.00.
    costUsd: records > 0 && unpricedRecords >= records ? null : costUsd,
    records: records,
    unpricedRecords: unpricedRecords,
  );
}

String _usageDay(DateTime day) =>
    '${day.year.toString().padLeft(4, '0')}-'
    '${day.month.toString().padLeft(2, '0')}-'
    '${day.day.toString().padLeft(2, '0')}';
