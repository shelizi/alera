import 'dart:convert';
import 'dart:collection';
import 'dart:io';

import 'package:alera/src/features/agent_status/domain/agent_status.dart';

part 'normalizers/agent_hook_adapter.dart';
part 'normalizers/agy_agent_hook_normalizer.dart';
part 'normalizers/amp_agent_hook_normalizer.dart';
part 'normalizers/claude_agent_hook_normalizer.dart';
part 'normalizers/codex_agent_hook_normalizer.dart';
part 'normalizers/copilot_agent_hook_normalizer.dart';
part 'normalizers/cursor_agent_hook_normalizer.dart';
part 'normalizers/devin_agent_hook_normalizer.dart';
part 'normalizers/fx_agent_hook_normalizer.dart';
part 'normalizers/grok_agent_hook_normalizer.dart';
part 'normalizers/opencode_agent_hook_normalizer.dart';
part 'normalizers/opencode2_agent_hook_normalizer.dart';
part 'normalizers/pi_agent_hook_normalizer.dart';
part 'normalizers/agent_hook_tool_snapshot.dart';
part 'normalizers/agent_hook_tool_preview.dart';
part 'normalizers/agent_hook_transcript_reader.dart';
part 'normalizers/agent_hook_status_helpers.dart';

class const NormalizedAgentStatus({
  required final AgentStatusState state,
  required final String prompt,
  final String? toolName,
  final String? toolInput,
  final String? lastAssistantMessage,
  final bool? interrupted,
});

NormalizedAgentStatus? normalizeAgentHookEvent(
  AgentHookEvent event, {
  AgentStatusEntry? previous,
}) {
  final adapter = _agentHookAdapterFor(event.agentType);
  final eventName = agentHookEventName(event);
  if (eventName == null) {
    return null;
  }
  final toolSnapshot = _extractToolSnapshot(
    event,
    eventName: eventName,
    adapter: adapter,
  );
  final state = adapter.normalizeState(
    event,
    eventName,
    toolSnapshot.toolName,
    previous,
  );
  if (state == null) {
    return null;
  }

  final isNewTurn = adapter.isNewTurn(eventName);
  final prompt = adapter.promptForEvent(event, eventName);
  final interrupted = adapter.interruptedFor(event, eventName, state, previous);
  return NormalizedAgentStatus(
    state: state,
    prompt: prompt.isNotEmpty
        ? prompt
        : (isNewTurn ? '' : previous?.prompt ?? ''),
    toolName: isNewTurn ? null : toolSnapshot.toolName ?? previous?.toolName,
    toolInput: isNewTurn
        ? null
        : toolSnapshot.toolInput ??
              (toolSnapshot.hasToolUpdate ? null : previous?.toolInput),
    lastAssistantMessage:
        toolSnapshot.lastAssistantMessage ?? previous?.lastAssistantMessage,
    interrupted: interrupted,
  );
}

bool isAgentSessionCloseHookEvent(AgentHookEvent event) {
  final eventName = agentHookEventName(event);
  return eventName != null &&
      _agentHookAdapterFor(event.agentType).isSessionClose(eventName);
}

bool isAgentSessionResetHookEvent(AgentHookEvent event) {
  final eventName = agentHookEventName(event);
  return eventName != null &&
      _agentHookAdapterFor(event.agentType).isSessionReset(eventName);
}

bool isAgentNewTurnHookEvent(AgentHookEvent event) {
  final eventName = agentHookEventName(event);
  return eventName != null &&
      _agentHookAdapterFor(event.agentType).isNewTurn(eventName);
}

/// True only for an explicit cancellation/interruption signal from the agent.
///
/// This intentionally does not treat generic failure events as user
/// interruptions: a failed tool/turn may still be useful as a completed state,
/// while Ctrl+C/cancel should remove the stale attention state entirely.
bool isExplicitAgentInterruptHookEvent(AgentHookEvent event) {
  final eventName = agentHookEventName(event);
  return eventName != null &&
      _agentHookAdapterFor(event.agentType)
          .isExplicitInterrupt(event, eventName);
}

String? agentHookEventName(AgentHookEvent event) {
  final explicit = _readFirstString(
    <String, Object?>{'hookEventName': event.hookEventName},
    const <String>['hookEventName'],
  );
  final raw =
      explicit ??
      _readFirstString(event.payload, const <String>[
        'hook_event_name',
        'hookEventName',
        'hook_type',
        'hookType',
      ]);
  return _agentHookAdapterFor(event.agentType).normalizeEventName(event, raw);
}

String _extractPrompt(Map<String, Object?> payload) {
  return _readFirstString(payload, const <String>[
        'prompt',
        'user_prompt',
        'userPrompt',
        'initial_prompt',
        'initialPrompt',
        'user_message',
        'userMessage',
        'message',
      ]) ??
      '';
}
