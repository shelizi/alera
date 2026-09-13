import 'package:alera/src/features/agent_status/domain/agent_status.dart';

/// Mirrors `alera_core::agent_descriptor::AgentHookStrategy` until FRB exposes
/// the Rust descriptor table to Dart.
enum AgentHookStrategy {
  none,
  configJson,
  pluginScript,
  runtimeHome,
  sessionOverlay,
  herdrSocket,
}

/// Mirrors `alera_core::agent_descriptor::AgentStatusStrategy` until FRB
/// exposes the Rust descriptor table to Dart.
enum AgentStatusStrategy {
  hookEvents,
  hookEventsWithTranscriptWatch,
  herdrSocket,
}

const Map<AgentType, AgentHookStrategy> agentHookStrategies =
    <AgentType, AgentHookStrategy>{
      AgentType.codex: AgentHookStrategy.runtimeHome,
      AgentType.claude: AgentHookStrategy.runtimeHome,
      AgentType.copilot: AgentHookStrategy.configJson,
      AgentType.cursor: AgentHookStrategy.sessionOverlay,
      AgentType.agy: AgentHookStrategy.configJson,
      AgentType.opencode: AgentHookStrategy.pluginScript,
      AgentType.opencode2: AgentHookStrategy.pluginScript,
      AgentType.pi: AgentHookStrategy.pluginScript,
      AgentType.amp: AgentHookStrategy.pluginScript,
      AgentType.grok: AgentHookStrategy.configJson,
      AgentType.devin: AgentHookStrategy.configJson,
      AgentType.fx: AgentHookStrategy.herdrSocket,
    };

const Map<AgentType, AgentStatusStrategy> agentStatusStrategies =
    <AgentType, AgentStatusStrategy>{
      AgentType.codex: AgentStatusStrategy.hookEventsWithTranscriptWatch,
      AgentType.claude: AgentStatusStrategy.hookEvents,
      AgentType.copilot: AgentStatusStrategy.hookEvents,
      AgentType.cursor: AgentStatusStrategy.hookEvents,
      AgentType.agy: AgentStatusStrategy.hookEvents,
      AgentType.opencode: AgentStatusStrategy.hookEvents,
      AgentType.opencode2: AgentStatusStrategy.hookEvents,
      AgentType.pi: AgentStatusStrategy.hookEvents,
      AgentType.amp: AgentStatusStrategy.hookEvents,
      AgentType.grok: AgentStatusStrategy.hookEvents,
      AgentType.devin: AgentStatusStrategy.hookEvents,
      AgentType.fx: AgentStatusStrategy.herdrSocket,
    };

extension AgentDescriptorStrategies on AgentType {
  AgentHookStrategy get hookStrategy => agentHookStrategies[this]!;

  AgentStatusStrategy get statusStrategy => agentStatusStrategies[this]!;
}
