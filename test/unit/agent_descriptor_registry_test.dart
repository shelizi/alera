import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('agent descriptor registry', () {
    test('canonicalizes agent keys', () {
      expect(canonicalAgentKey('  CODEX '), 'codex');
      expect(canonicalAgentKey('antigravity'), 'agy');
      expect(canonicalAgentKey('nope'), 'nope');
    });

    test('looks up descriptors by canonical and alias ids', () {
      expect(agentDescriptorForKey('codex')?.id, 'codex');
      expect(agentDescriptorForKey('antigravity')?.id, 'agy');
    });

    test('has a descriptor for every agent type', () {
      for (final type in AgentType.values) {
        expect(agentDescriptorFor(type).id, type.key);
      }
    });
  });
}
