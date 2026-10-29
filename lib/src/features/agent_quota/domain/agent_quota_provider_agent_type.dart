import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';

/// Quota providers and agents share canonical ids when the provider is an
/// agent. External quota-only providers deliberately return null.
AgentType? agentTypeForQuotaProvider(AgentQuotaProviderId provider) {
  for (final agent in AgentType.values) {
    if (agent.key == provider.name) {
      return agent;
    }
  }
  return null;
}
