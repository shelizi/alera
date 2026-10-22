import 'package:alera/src/features/agent_quota/application/agent_quota_api_equivalent_summary.dart';
import 'package:alera/src/features/agent_usage/domain/agent_usage.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('keeps API-equivalent cost isolated by provider, account and calendar window', () {
    final usage = _usage([
      _bucket('2026-10-22', 2.50),
      _bucket('2026-10-19', 3.00),
      _bucket('2026-10-02', 7.00),
      _bucket('2026-09-30', 30.00),
      _bucket('2026-10-22', 40.00, account: 'work'),
      _bucket('2026-10-22', 10.00, provider: .codex),
      _bucket('2026-10-23', 99.00),
    ]);
    final claude = summarizeAgentQuotaApiEquivalentCost(
      usage: usage,
      provider: .claude,
      accountId: 'default',
      now: DateTime(2026, 10, 22),
    )!;
    expect(claude.today.costUsd, 2.50);
    expect(claude.lastSevenDays.costUsd, 5.50);
    expect(claude.thisMonth.costUsd, 12.50);
    final work = summarizeAgentQuotaApiEquivalentCost(
      usage: usage,
      provider: .claude,
      accountId: 'work',
      now: DateTime(2026, 10, 22),
    )!;
    expect(work.thisMonth.costUsd, 40.00);
    final codex = summarizeAgentQuotaApiEquivalentCost(
      usage: usage,
      provider: .codex,
      accountId: 'default',
      now: DateTime(2026, 10, 22),
    )!;
    expect(codex.thisMonth.costUsd, 10.00);
  });

  test('distinguishes unpriced sessions from actual zero cost', () {
    final usage = _usage([
      _bucket('2026-10-22', 0, records: 2, unpriced: 2),
      _bucket('2026-10-21', 1.25),
    ]);
    final summary = summarizeAgentQuotaApiEquivalentCost(
      usage: usage,
      provider: .claude,
      accountId: 'default',
      now: DateTime(2026, 10, 22),
    )!;
    expect(summary.today.costUsd, isNull);
    expect(summary.lastSevenDays.costUsd, 1.25);
    expect(summary.thisMonth.costUsd, 1.25);
    expect(summary.hasUnpricedUsage, isTrue);
  });

  test('covers seven days across the month boundary without adding September to October', () {
    final usage = _usage([
      _bucket('2026-10-01', 2),
      _bucket('2026-09-29', 3),
      _bucket('2026-09-23', 9),
    ]);
    final summary = summarizeAgentQuotaApiEquivalentCost(
      usage: usage,
      provider: .claude,
      accountId: 'default',
      now: DateTime(2026, 10, 1),
    )!;
    expect(summary.today.costUsd, 2);
    expect(summary.lastSevenDays.costUsd, 5);
    expect(summary.thisMonth.costUsd, 2);
  });

  test('does not invent a cost for an account with no recorded source', () {
    final usage = _usage([_bucket('2026-10-22', 3)]);
    expect(
      summarizeAgentQuotaApiEquivalentCost(
        usage: usage,
        provider: .claude,
        accountId: 'missing',
        now: DateTime(2026, 10, 22),
      ),
      isNull,
    );
  });
}

AgentUsageBucket _bucket(
  String day,
  double cost, {
  AgentUsageProvider provider = .claude,
  String account = 'default',
  int records = 1,
  int unpriced = 0,
}) => AgentUsageBucket(
  day: day,
  provider: provider,
  accountId: account,
  displayName: account,
  model: 'test-model',
  totals: AgentUsageTokenTotals.zero,
  costUsd: cost,
  cacheSavingsUsd: 0,
  costSource: unpriced == records ? .unpriced : .modelPriced,
  records: records,
  unpricedRecords: unpriced,
  sessions: 1,
);

AgentUsageSnapshot _usage(List<AgentUsageBucket> buckets) => AgentUsageSnapshot(
  readAt: DateTime.utc(2026, 10, 22),
  sinceDay: '2026-09-22',
  untilDay: '2026-10-22',
  buckets: buckets,
  sources: [
    for (final provider in [
      AgentUsageProvider.claude,
      AgentUsageProvider.codex,
    ])
      for (final account in ['default', 'work'])
        AgentUsageSource(
          provider: provider,
          accountId: account,
          displayName: account,
          status: .ok,
          scannedFiles: 1,
          skippedFiles: 0,
          distinctSessions: 1,
          message: null,
        ),
  ],
  pricing: const AgentUsagePricing(
    status: .fresh,
    source: 'fixture',
    knownModels: 1,
  ),
  scanDurationMs: 1,
);
