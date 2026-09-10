import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'runtime_agent_status_sync.g.dart';

const String _agentPresenceCoalesceKey = 'agentPresence';

final class RuntimeAgentPresenceDelta {
  const RuntimeAgentPresenceDelta({
    required this.upserts,
    required this.removedSessionIds,
  });

  final List<AgentStatusEntry> upserts;
  final Set<String> removedSessionIds;
}

/// Decodes the compact payload emitted by newer runtime hosts.
///
/// A null result means the event came from an older host, or from a global
/// invalidation that intentionally requires a full snapshot reconciliation.
RuntimeAgentPresenceDelta? decodeRuntimeAgentPresenceDelta(
  Map<String, Object?> payload,
) {
  final changes = payload['changes'];
  if (changes is! List) {
    return null;
  }
  final upserts = <AgentStatusEntry>[];
  final removedSessionIds = <String>{};
  for (final raw in changes.whereType<Map>()) {
    final change = Map<String, Object?>.from(raw);
    final sessionId = change['terminalSessionId'];
    if (sessionId is! String || sessionId.isEmpty) {
      continue;
    }
    if (change['removed'] == true) {
      removedSessionIds.add(sessionId);
      continue;
    }
    if (_entryFromRuntime(change) case final entry?) {
      upserts.add(entry);
    }
  }
  return RuntimeAgentPresenceDelta(
    upserts: upserts,
    removedSessionIds: removedSessionIds,
  );
}

@Riverpod(keepAlive: true)
void runtimeAgentStatusSync(Ref ref) {
  final client = ref.watch(runtimeHostClientProvider);
  final coalescer = ref.watch(runtimeChangeCoalescerProvider);
  final coalesceOwner = Object();
  var disposed = false;
  var generation = 0;

  Future<void> refresh() async {
    final requested = ++generation;
    try {
      final payload = await client.runtimeRequest('agentPresence.list');
      // Concurrent refreshes can land out of order, and an older snapshot
      // would undo a newer one.
      if (disposed || requested != generation) {
        return;
      }
      final entries = payload is List
          ? payload
                .whereType<Map>()
                .map(
                  (item) => _entryFromRuntime(Map<String, Object?>.from(item)),
                )
                .whereType<AgentStatusEntry>()
          : const <AgentStatusEntry>[];
      ref
          .read(agentStatusControllerProvider.notifier)
          .replaceRuntimeSnapshot(entries);
    } on Object {
      // Older sidecars keep the existing local status implementation working.
    }
  }

  final subscription = client.runtimeEvents.listen((event) {
    // New hosts include per-session changes so background tabs update without
    // a snapshot RPC. Empty legacy/global events still fall back to a
    // coalesced full reconciliation.
    if (event.name == 'agentPresenceChanged') {
      final delta = decodeRuntimeAgentPresenceDelta(event.payload);
      if (delta != null) {
        ref
            .read(agentStatusControllerProvider.notifier)
            .mergeRuntimeDelta(
              upserts: delta.upserts,
              removedSessionIds: delta.removedSessionIds,
            );
      } else {
        coalescer.schedule(_agentPresenceCoalesceKey, coalesceOwner, refresh);
      }
    } else if (event.name == aleraRuntimeHostConnectedEvent) {
      // Reconnecting means the local snapshot may be stale in either
      // direction, so resync now instead of waiting out the debounce.
      coalescer.cancel(_agentPresenceCoalesceKey, coalesceOwner);
      unawaited(refresh());
    }
  });
  ref.onDispose(() {
    disposed = true;
    coalescer.cancel(_agentPresenceCoalesceKey, coalesceOwner);
    unawaited(subscription.cancel());
  });
  unawaited(refresh());
}

AgentStatusEntry? _entryFromRuntime(Map<String, Object?> json) {
  final sessionId = json['handle'] ?? json['terminalSessionId'];
  final workspaceId = json['workspaceId'];
  final tabId = json['tabId'];
  final agentType = AgentType.values
      .where((value) => value.key == json['agentType'])
      .firstOrNull;
  final state = AgentStatusState.values
      .where((value) => value.key == (json['agentState'] ?? json['state']))
      .firstOrNull;
  if (sessionId is! String ||
      workspaceId is! String ||
      tabId is! String ||
      agentType == null ||
      state == null) {
    return null;
  }
  final updatedAt =
      DateTime.tryParse(json['updatedAt'] as String? ?? '')?.toUtc() ??
      DateTime.now().toUtc();
  final stateStartedAt =
      DateTime.tryParse(json['stateStartedAt'] as String? ?? '')?.toUtc() ??
      updatedAt;
  return AgentStatusEntry(
    terminalSessionId: sessionId,
    workspaceId: workspaceId,
    tabId: tabId,
    agentType: agentType,
    state: state,
    prompt: json['prompt'] as String? ?? '',
    updatedAt: updatedAt,
    stateStartedAt: stateStartedAt,
    toolName: json['toolName'] as String?,
    toolInput: json['toolInput'] as String?,
    lastAssistantMessage: json['lastAssistantMessage'] as String?,
    interrupted: json['interrupted'] as bool?,
  );
}
