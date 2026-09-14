import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';

/// Canonicalizes a wire or config agent id the same way `canonical_agent_id`
/// does in `rust/alera-core/src/agent_descriptor.rs`: trim, lowercase, then
/// alias resolution (`antigravity` resolves to `agy`). Unknown ids pass
/// through unchanged.
String canonicalAgentKey(String key) {
  final normalized = key.trim().toLowerCase();
  for (final descriptor in agentDescriptorSnapshots) {
    if (descriptor.id == normalized ||
        descriptor.aliases.contains(normalized)) {
      return descriptor.id;
    }
  }
  return normalized;
}

/// Looks up a descriptor by canonical or alias id. Returns null for ids the
/// Rust descriptor table does not know about.
AgentDescriptorSnapshot? agentDescriptorForKey(String key) {
  final canonical = canonicalAgentKey(key);
  for (final descriptor in agentDescriptorSnapshots) {
    if (descriptor.id == canonical) {
      return descriptor;
    }
  }
  return null;
}

/// The descriptor for a typed adapter. Every `AgentType` has a table entry;
/// a missing entry means the checked-in snapshot drifted from the Rust table,
/// which `agent_descriptor_snapshot_is_fresh` exists to prevent.
AgentDescriptorSnapshot agentDescriptorFor(AgentType type) {
  final descriptor = agentDescriptorForKey(type.key);
  if (descriptor == null) {
    throw StateError('missing agent descriptor for ${type.key}');
  }
  return descriptor;
}

/// Agent types whose status capability is backed by the status subsystem.
final List<AgentType> agentTypesWithStatusHooks = List<AgentType>.unmodifiable(
  agentDescriptorSnapshots
      .where(
        (descriptor) =>
            descriptor.statusStrategy ==
                AgentStatusStrategySnapshot.hookEvents ||
            descriptor.statusStrategy ==
                AgentStatusStrategySnapshot.hookEventsWithTranscriptWatch ||
            descriptor.statusStrategy == AgentStatusStrategySnapshot.herdrSocket,
      )
      .map(_agentTypeForDescriptor),
);

/// Agent types that receive status through the HTTP hook receiver.
final List<AgentType> agentTypesWithHttpHooks = List<AgentType>.unmodifiable(
  agentDescriptorSnapshots
      .where(
        (descriptor) =>
            descriptor.statusStrategy ==
                AgentStatusStrategySnapshot.hookEvents ||
            descriptor.statusStrategy ==
                AgentStatusStrategySnapshot.hookEventsWithTranscriptWatch,
      )
      .map(_agentTypeForDescriptor),
);

/// Agent types reconciled through the shared managed-hook installer.
/// Config-json agents whose persistent user-home hooks are owned by the shared
/// managed-hook reconciler. Copilot uses ConfigJson too, but its lifecycle is
/// not owned by this reconciler.
const Set<String> _globalManagedHookAgentIds = <String>{'agy', 'grok', 'devin'};

final List<AgentType> agentTypesWithGlobalManagedHooks =
    List<AgentType>.unmodifiable(
      agentDescriptorSnapshots
          .where((descriptor) => _globalManagedHookAgentIds.contains(descriptor.id))
          .map(_agentTypeForDescriptor),
    );

bool isAgentSettingEnabled(Map<String, bool> settings, AgentType agentType) {
  final canonical = canonicalAgentKey(agentType.key);
  if (settings.containsKey(canonical)) {
    return settings[canonical] ?? false;
  }
  final descriptor = agentDescriptorFor(agentType);
  for (final alias in descriptor.aliases) {
    if (settings.containsKey(alias)) {
      return settings[alias] ?? false;
    }
  }
  return false;
}

AgentType _agentTypeForDescriptor(AgentDescriptorSnapshot descriptor) {
  return AgentType.values.firstWhere((type) => type.key == descriptor.id);
}
