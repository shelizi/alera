import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/ai_assist/presentation/ai_assist_agent_choice.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('AI Assist agent choice keeps global custom and agent distinct', () {
    expect(const AiAssistAgentChoice.global().isGlobal, isTrue);
    expect(const AiAssistAgentChoice.custom().isCustom, isTrue);
    expect(
      const AiAssistAgentChoice.agent(AgentType.devin).agentType,
      AgentType.devin,
    );
    expect(
      const AiAssistAgentChoice.agent(AgentType.devin),
      const AiAssistAgentChoice.agent(AgentType.devin),
    );
    expect(
      const AiAssistAgentChoice.custom(),
      isNot(const AiAssistAgentChoice.global()),
    );
  });
}
