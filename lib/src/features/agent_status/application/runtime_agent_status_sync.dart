import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
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
  Map<String, Object?> payload, {
  Map<String, AgentStatusEntry> previous = const {},
}) {
  final changes = payload['changes'];
  if (changes is! List) {
    return null;
  }
  final upserts = <AgentStatusEntry>[];
  final removedSessionIds = <String>{};
  for (final change in changes.whereType<Map>()) {
    final sessionId = change['terminalSessionId'];
    if (sessionId is! String || sessionId.isEmpty) {
      continue;
    }
    if (change['removed'] == true) {
      removedSessionIds.add(sessionId);
      continue;
    }
    if (_entryFromRuntime(change, previous: previous) case final entry?) {
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
  Completer<void>? refreshing;
  var refreshQueued = false;

  Future<void> refresh() async {
    if (disposed) return;
    if (refreshing case final active?) {
      // Reconnect and legacy invalidations can arrive during a request.
      // Keep only one follow-up; never enqueue concurrent list RPCs.
      generation += 1;
      refreshQueued = true;
      return active.future;
    }
    final completion = Completer<void>();
    refreshing = completion;
    try {
      do {
        refreshQueued = false;
        final requested = ++generation;
        try {
          final payload = await client.runtimeRequest('agentPresence.list');
          // A delta or queued refresh invalidates a snapshot in flight.
          if (disposed || requested != generation) continue;
          final previous = ref.read(agentStatusControllerProvider);
          final entries = payload is List
              ? payload
                    .whereType<Map>()
                    .map((item) => _entryFromRuntime(item, previous: previous))
                    .whereType<AgentStatusEntry>()
              : const <AgentStatusEntry>[];
          ref
              .read(agentStatusControllerProvider.notifier)
              .replaceRuntimeSnapshot(entries);
        } on Object {
          // Older sidecars keep the existing local status implementation working.
        }
      } while (refreshQueued && !disposed);
    } finally {
      refreshing = null;
      completion.complete();
    }
  }

  final subscription = client.runtimeEvents.listen((event) {
    // New hosts include per-session changes so background tabs update without
    // a snapshot RPC. Empty legacy/global events still fall back to a
    // coalesced full reconciliation.
    if (event.name == 'agentPresenceChanged') {
      final delta = decodeRuntimeAgentPresenceDelta(
        event.payload,
        previous: ref.read(agentStatusControllerProvider),
      );
      if (delta != null) {
        // The event is newer than every snapshot request already in flight.
        // Invalidate those requests before applying the delta so a late list
        // response cannot replace this newer per-session state.
        generation += 1;
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

final _agentTypesByKey = {for (final type in AgentType.values) type.key: type};
final _agentStatesByKey = {
  for (final state in AgentStatusState.values) state.key: state,
};

AgentStatusEntry? _entryFromRuntime(
  Map json, {
  required Map<String, AgentStatusEntry> previous,
}) {
  final sessionId = json['handle'] ?? json['terminalSessionId'];
  final workspaceId = json['workspaceId'];
  final tabId = json['tabId'];
  final agentType = _agentTypesByKey[json['agentType']];
  final state = _agentStatesByKey[json['agentState'] ?? json['state']];
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
  final old = previous[sessionId];
  // Do not reuse data from a different workspace, tab or agent occupying
  // the same terminal handle. Only identical validated input is reused.
  final matching =
      old != null &&
          old.terminalSessionId == sessionId &&
          old.workspaceId == workspaceId &&
          old.tabId == tabId &&
          old.agentType == agentType
      ? old
      : null;
  final prompt = json['prompt'] as String? ?? '';
  final toolName = json['toolName'] as String?;
  final toolInput = json['toolInput'] as String?;
  final lastAssistantMessage = json['lastAssistantMessage'] as String?;
  final interrupted = json['interrupted'] as bool?;
  final rawContext = json['contextOverview'];
  final context = rawContext is Map
      ? AgentContextOverview.snapshotFromRaw(
          rawContext,
          previous: matching?.contextOverview,
        )
      : null;
  if (matching != null &&
      matching.state == state &&
      matching.updatedAt == updatedAt &&
      matching.stateStartedAt == stateStartedAt &&
      matching.prompt == prompt &&
      matching.toolName == toolName &&
      matching.toolInput == toolInput &&
      matching.lastAssistantMessage == lastAssistantMessage &&
      matching.interrupted == interrupted &&
      identical(matching.contextOverview, context)) {
    return matching;
  }
  return AgentStatusEntry(
    terminalSessionId: sessionId,
    workspaceId: workspaceId,
    tabId: tabId,
    agentType: agentType,
    state: state,
    prompt: prompt,
    updatedAt: updatedAt,
    stateStartedAt: stateStartedAt,
    toolName: toolName,
    toolInput: toolInput,
    lastAssistantMessage: lastAssistantMessage,
    interrupted: interrupted,
    contextOverview: context,
  );
}
