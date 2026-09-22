import 'package:alera/src/features/agent_status/domain/agent_status.dart';

enum AiAssistAgentChoiceKind { global, custom, agent }

class AiAssistAgentChoice {
  const AiAssistAgentChoice.global()
    : kind = AiAssistAgentChoiceKind.global,
      agentType = null;

  const AiAssistAgentChoice.custom()
    : kind = AiAssistAgentChoiceKind.custom,
      agentType = null;

  const AiAssistAgentChoice.agent(AgentType this.agentType)
    : kind = AiAssistAgentChoiceKind.agent;

  final AiAssistAgentChoiceKind kind;
  final AgentType? agentType;

  bool get isGlobal => kind == AiAssistAgentChoiceKind.global;
  bool get isCustom => kind == AiAssistAgentChoiceKind.custom;
  bool get isAgent => kind == AiAssistAgentChoiceKind.agent;

  String get key => switch (kind) {
    AiAssistAgentChoiceKind.global => 'global',
    AiAssistAgentChoiceKind.custom => 'custom',
    AiAssistAgentChoiceKind.agent => agentType!.key,
  };

  @override
  bool operator ==(Object other) =>
      other is AiAssistAgentChoice &&
      other.kind == kind &&
      other.agentType == agentType;

  @override
  int get hashCode => Object.hash(kind, agentType);
}
