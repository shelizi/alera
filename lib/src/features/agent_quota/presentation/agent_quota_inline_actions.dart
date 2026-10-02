import 'package:alera/src/features/agent_quota/domain/agent_quota.dart';
import 'package:flutter/widgets.dart';

typedef AgentQuotaActionBuilder = Widget Function({
  required String hostId,
  required AgentQuotaSnapshot snapshot,
  required bool compact,
});

/// Optional runtime controls supplied by the native feature wrapper.
class const AgentQuotaInlineActions({
  final AgentQuotaActionBuilder? codexReset,
  final AgentQuotaActionBuilder? claudeTui,
  final AgentQuotaActionBuilder? accountSwitch,
}) {
  Widget buildAccountSwitch({
    required String hostId,
    required AgentQuotaSnapshot snapshot,
  }) =>
      accountSwitch?.call(hostId: hostId, snapshot: snapshot, compact: false) ??
      const SizedBox.shrink();

  Widget buildCodexReset({
    required String hostId,
    required AgentQuotaSnapshot snapshot,
    bool compact = false,
  }) =>
      codexReset?.call(hostId: hostId, snapshot: snapshot, compact: compact) ??
      const SizedBox.shrink();

  Widget buildClaudeTui({
    required String hostId,
    required AgentQuotaSnapshot snapshot,
  }) =>
      claudeTui?.call(hostId: hostId, snapshot: snapshot, compact: false) ??
      const SizedBox.shrink();
}
