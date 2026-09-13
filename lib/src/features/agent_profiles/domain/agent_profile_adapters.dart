import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';

/// The agent adapters a profile may target.
///
/// The checked-in descriptor snapshot is generated from the Rust registry and
/// preserves its order.
final List<AgentType> spawnableAgentProfileAdapters = <AgentType>[
  for (final descriptor in agentDescriptorSnapshots)
    AgentType.values.firstWhere((adapter) => adapter.key == descriptor.id),
];

/// The default launch command each adapter uses when a profile does not
/// override it. Reads `defaultCommand` from the checked-in descriptor
/// snapshot.
final Map<AgentType, String> agentProfileDefaultCommands = <AgentType, String>{
  for (final adapter in AgentType.values)
    adapter: agentDescriptorFor(adapter).defaultCommand,
};

AgentType? agentProfileAdapterFromKey(String key) {
  final canonical = canonicalAgentKey(key);
  for (final adapter in spawnableAgentProfileAdapters) {
    if (adapter.key == canonical) {
      return adapter;
    }
  }
  return null;
}
