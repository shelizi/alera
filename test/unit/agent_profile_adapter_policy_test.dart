import 'package:alera/src/features/agent_profiles/application/agent_profile_persona_discovery.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_quota/domain/agent_quota_provider_agent_type.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('new profile defaults to the canonical Codex adapter', () {
    expect(defaultAgentProfileAdapter, AgentType.codex);
    expect(spawnableAgentProfileAdapters, contains(defaultAgentProfileAdapter));
  });

  test('persona discovery and initial suggestions remain agent-owned', () {
    for (final adapter in AgentType.values) {
      final enabled = <AgentType>{
        AgentType.agy,
        AgentType.opencode,
        AgentType.opencode2,
      }.contains(adapter);
      expect(
        AgentProfilePersonaDiscovery.supportsAdapter(adapter),
        enabled,
        reason: adapter.key,
      );
      expect(
        AgentProfilePersonaDiscovery.defaultPersonasFor(adapter),
        adapter == AgentType.opencode || adapter == AgentType.opencode2
            ? <String>['build']
            : isEmpty,
      );
    }
  });

  test(
    'quota providers with canonical agent ids resolve without a UI switch',
    () {
      for (final provider in AgentQuotaProviderId.values) {
        final expected = AgentType.values.where(
          (adapter) => adapter.key == provider.name,
        );
        expect(
          agentTypeForQuotaProvider(provider),
          expected.isEmpty ? isNull : expected.single,
          reason: provider.name,
        );
      }
    },
  );
}
