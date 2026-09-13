import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';

/// Compatibility enum retained for existing hook installation callers.
enum AgentHookStrategy {
  none,
  configJson,
  pluginScript,
  runtimeHome,
  sessionOverlay,
  herdrSocket,
}

/// Compatibility enum retained for existing status normalization callers.
enum AgentStatusStrategy {
  hookEvents,
  hookEventsWithTranscriptWatch,
  herdrSocket,
}

extension AgentDescriptorStrategies on AgentType {
  AgentHookStrategy get hookStrategy => AgentHookStrategy.values.byName(
    agentDescriptorFor(this).hookStrategy.name,
  );

  AgentStatusStrategy get statusStrategy => AgentStatusStrategy.values.byName(
    agentDescriptorFor(this).statusStrategy.name,
  );
}
