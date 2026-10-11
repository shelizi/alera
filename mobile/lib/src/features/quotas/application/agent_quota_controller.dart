import 'dart:async';

import 'package:alera_mobile/src/features/quotas/domain/quota_snapshot.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'agent_quota_controller.g.dart';

const Duration mobileQuotaRefreshInterval = Duration(minutes: 5);

@riverpod
class AgentQuotaController extends _$AgentQuotaController {
  @override
  Future<QuotaSnapshotState> build(String hostId) async {
    final client = await watchHostConnection(ref, hostId);
    if (!client.supportsAgentQuotas) {
      throw UnsupportedError('Update the runtime to view quotas.');
    }
    if (!ref.mounted) {
      return client.fetchAgentQuotas();
    }
    final timer = Timer(mobileQuotaRefreshInterval, () {
      if (ref.mounted) {
        ref.invalidateSelf();
      }
    });
    ref.onDispose(timer.cancel);
    return client.fetchAgentQuotas();
  }

  Future<void> refresh() async {
    final client = await ref.read(
      hostConnectionControllerProvider(hostId).future,
    );
    if (!ref.mounted) {
      return;
    }
    state = const AsyncLoading<QuotaSnapshotState>();
    final nextState = await AsyncValue.guard(
      () => client.fetchAgentQuotas(forceRefresh: true),
    );
    if (ref.mounted) {
      state = nextState;
    }
  }

  /// New hosts refresh only the selected account; older hosts fall back to a
  /// full fetch while preserving the other cards' last known values.
  Future<void> refreshAccount(QuotaSnapshot account) async {
    final client = await ref.read(
      hostConnectionControllerProvider(hostId).future,
    );
    final refreshed = client.supportsAgentQuotaPartialRefresh
        ? await client.fetchAgentQuotaAccount(
            provider: account.provider,
            accountId: account.accountId,
          )
        : await client.fetchAgentQuotas(forceRefresh: true);
    if (!ref.mounted) return;
    final previous = state.value;
    if (previous == null) {
      state = AsyncData(refreshed);
      return;
    }
    state = AsyncData(mergeRefreshedQuotaAccount(previous, refreshed, account));
  }

  Future<void> tryClaudeWithTui(QuotaSnapshot snapshot) async {
    final client = await ref.read(
      hostConnectionControllerProvider(hostId).future,
    );
    if (!client.supportsAgentQuotaClaudeTui) {
      throw UnsupportedError('Update the runtime to try Claude with TUI.');
    }
    final updated = await client.fetchClaudeQuotaViaTui(
      accountId: snapshot.accountId,
      displayName: snapshot.displayName,
    );
    if (!ref.mounted) {
      return;
    }
    final previous = state.value;
    if (previous == null) {
      ref.invalidateSelf();
      return;
    }
    final snapshots = <QuotaSnapshot>[
      for (final item in previous.snapshots)
        if (item.provider == updated.provider &&
            item.accountId == updated.accountId)
          updated
        else
          item,
    ];
    if (!snapshots.any(
      (item) =>
          item.provider == updated.provider &&
          item.accountId == updated.accountId,
    )) {
      snapshots.add(updated);
    }
    state = AsyncData(
      QuotaSnapshotState(
        snapshots: snapshots,
        environment: previous.environment,
        fetchedAt: DateTime.now().toUtc(),
      ),
    );
  }

  Future<CodexResetConsumeResult> consumeCodexResetCredit(
    QuotaSnapshot snapshot,
  ) async {
    final credits = snapshot.rateLimitResetCredits;
    if (credits == null || !credits.canConsume) {
      throw StateError('No Codex reset credit is available.');
    }
    final client = await ref.read(
      hostConnectionControllerProvider(hostId).future,
    );
    if (!client.supportsCodexResetCredits) {
      throw UnsupportedError('Update the runtime to use Codex resets.');
    }
    final result = await client.consumeCodexResetCredit(credits.offerRevision);
    if (!ref.mounted) {
      return result;
    }
    final previous = state.value;
    if (previous == null) {
      ref.invalidateSelf();
      return result;
    }
    final snapshots = <QuotaSnapshot>[
      for (final item in previous.snapshots)
        if (item.provider == result.snapshot.provider &&
            item.accountId == result.snapshot.accountId)
          result.snapshot
        else
          item,
    ];
    state = AsyncData(
      QuotaSnapshotState(
        snapshots: snapshots,
        environment: previous.environment,
        fetchedAt: DateTime.now().toUtc(),
      ),
    );
    return result;
  }
}

QuotaSnapshotState mergeRefreshedQuotaAccount(
  QuotaSnapshotState previous,
  QuotaSnapshotState refreshed,
  QuotaSnapshot account,
) {
  final matches = <QuotaSnapshot>[
    for (final snapshot in refreshed.snapshots)
      if (snapshot.provider == account.provider &&
          snapshot.accountId == account.accountId)
        snapshot,
  ];
  if (matches.isEmpty) return previous;
  return QuotaSnapshotState(
    snapshots: <QuotaSnapshot>[
      for (final snapshot in previous.snapshots)
        if (snapshot.provider == account.provider &&
            snapshot.accountId == account.accountId)
          matches.first
        else
          snapshot,
    ],
    environment: previous.environment,
    fetchedAt: previous.fetchedAt,
  );
}
