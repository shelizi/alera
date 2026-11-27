import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

typedef AgentContextIdentity = ({
  String workspaceId,
  String tabId,
  String sessionId,
  String conversationId,
  String provider,
});

class AgentContextRecovery {
  const AgentContextRecovery(this.overview, {this.fromHistory = false});
  final AgentContextOverview overview;
  final bool fromHistory;
}

/// A registered Agent tab is still an Agent tab when live presence is empty.
String? agentContextProviderForTab(WorkspaceTabRecord tab) {
  final snapshot = tab.payload['agentProfileLaunchV1'];
  final pending = tab.payload['pendingAgentPrompt'];
  final spawn = tab.payload['orchestrationSpawn'];
  final candidate =
      (snapshot is Map ? snapshot['agentType'] : null) ??
      tab.payload['agentType'] ??
      (pending is Map ? pending['agent'] : null) ??
      (spawn is Map ? spawn['agent'] : null);
  return candidate is String && candidate.isNotEmpty ? candidate : null;
}

/// Only local Host-verified sources are used. Old Hosts may not expose
/// checkpoints; their exact read-only history API is still useful. No Agent
/// invocation, terminal output heuristics, or caller-supplied file paths.
final agentContextRecoveryProvider = FutureProvider.autoDispose
    .family<AgentContextRecovery, AgentContextIdentity>((ref, identity) async {
      final host = ref.watch(runtimeHostClientProvider);
      var saved = const AgentContextOverview();
      var fromHistory = false;
      try {
        final raw = await host.runtimeRequest('agentContext.get', {
          'workspaceId': identity.workspaceId,
          'tabId': identity.tabId,
          'conversationId': identity.conversationId,
          'sessionId': identity.sessionId,
        });
        if (raw is Map && raw['readOnly'] == true) {
          saved = AgentContextOverview.fromRaw(raw['contextOverview']);
          fromHistory = raw['fromHistory'] == true;
        }
      } catch (_) {
        // A previous Host may not support this additive RPC.
      }
      if (saved.progress.isNotEmpty ||
          !const {'codex', 'claude'}.contains(identity.provider)) {
        return AgentContextRecovery(saved, fromHistory: fromHistory);
      }
      try {
        final raw = await host.runtimeRequest('agentHistory.messages', {
          'workspaceId': identity.workspaceId,
          'tabId': identity.tabId,
          'maxMessages': 30,
          'consentToReadMessages': true,
          'conversationId': identity.conversationId,
          'sessionId': identity.sessionId,
        }, const Duration(seconds: 45));
        if (raw is! Map ||
            raw['readOnly'] != true ||
            raw['source'] != 'nativeProvider') {
          return AgentContextRecovery(saved);
        }
        final messages = raw['messages'];
        if (messages is! List || messages.length > 60) {
          return AgentContextRecovery(saved);
        }
        final history = contextFromNativeMessages(messages);
        return AgentContextRecovery(
          AgentContextOverview(
            goal: saved.goal.isNotEmpty ? saved.goal : history.goal,
            initialGoal: saved.initialGoal.isNotEmpty
                ? saved.initialGoal
                : history.initialGoal,
            currentFocus: saved.currentFocus.isNotEmpty
                ? saved.currentFocus
                : history.currentFocus,
            progress: history.progress,
            reportSummary: saved.reportSummary,
            overallSummary: saved.overallSummary,
            timeline: mergeAgentContextTimelines(
              saved.timeline,
              history.timeline,
            ),
          ),
          fromHistory: history.progress.isNotEmpty,
        );
      } catch (_) {
        return AgentContextRecovery(saved);
      }
    });

/// The latest page contains only verified, human-visible native messages.
/// It is not evidence of task completion. Do not fabricate timestamps.
AgentContextOverview contextFromNativeMessages(List<dynamic> messages) {
  var context = const AgentContextOverview();
  var lastPrompt = '';
  var latestReply = '';
  for (final item in messages) {
    if (item is! Map || item['text'] is! String || item['role'] is! String) {
      continue;
    }
    final role = item['role'];
    final value = (item['text'] as String).trim();
    if (value.isEmpty ||
        value.runes.length > 8000 ||
        (role != 'user' && role != 'assistant')) {
      continue;
    }
    final timestamp = item['timestamp'];
    final at = timestamp is String ? DateTime.tryParse(timestamp) : null;
    if (role == 'user') lastPrompt = value;
    if (role == 'assistant') latestReply = value;
    if (at != null) {
      context = AgentContextOverview.observe(
        previous: context,
        previousPrompt: null,
        previousAssistant: null,
        prompt: role == 'user' ? value : '',
        assistant: role == 'assistant' ? value : null,
        at: at,
      );
    }
  }
  // A provider may omit timestamps. Show only its actual text, not fabricated
  // dated events. Normalize and bound through the same overview parser.
  return AgentContextOverview.fromRaw({
    ...context.toJson(),
    'goal': context.goal.isNotEmpty ? context.goal : lastPrompt,
    'progress': latestReply.isNotEmpty
        ? String.fromCharCodes(latestReply.runes.take(420))
        : context.progress,
  });
}

List<AgentContextMilestone> mergeAgentContextTimelines(
  List<AgentContextMilestone> live,
  List<AgentContextMilestone> recovered, {
  bool preferRecoveredSummaries = false,
}) {
  final result = <AgentContextMilestone>[];
  final seen = <(String, String, String), int>{};
  for (final event in [...live, ...recovered]) {
    final key = (event.kind, event.text, event.reportAt);
    final index = seen[key];
    if (index == null) {
      seen[key] = result.length;
      result.add(event);
    } else if (event.summary.isNotEmpty &&
        (result[index].summary.isEmpty || preferRecoveredSummaries)) {
      result[index] = event;
    }
  }
  result.sort((a, b) => b.at.compareTo(a.at));
  return result.take(16).toList(growable: false);
}
