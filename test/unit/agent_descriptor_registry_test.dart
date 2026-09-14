import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/application/agent_hook_reconciliation_service.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
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

    test('derives status and HTTP hook agent sets from descriptors', () {
      expect(
        agentTypesWithStatusHooks.toSet(),
        AgentType.values.toSet(),
      );
      expect(
        agentTypesWithHttpHooks.toSet(),
        <AgentType>{
          .codex,
          .claude,
          .copilot,
          .cursor,
          .agy,
          .opencode,
          .opencode2,
          .pi,
          .amp,
          .grok,
          .devin,
        },
      );
      expect(
        agentTypesWithGlobalManagedHooks.toSet(),
        <AgentType>{.agy, .grok, .devin},
      );
    });

    test('looks up settings by canonical agent key', () {
      expect(
        isAgentSettingEnabled(const {'antigravity': true}, .agy),
        isTrue,
      );
      expect(isAgentSettingEnabled(const {'codex': true}, .codex), isTrue);
      expect(isAgentSettingEnabled(const {'codex': false}, .codex), isFalse);
      expect(isAgentSettingEnabled(const {}, .codex), isFalse);
    });

    test('preserves the enabled status agent set', () {
      final enabled = enabledAgentStatusHookTypes(
        const AgentStatusHookSettings(values: <String, bool>{
          'codex': true,
          'claude': true,
          'copilot': true,
          'cursor': true,
          'agy': true,
          'opencode': true,
          'opencode2': true,
          'pi': true,
          'amp': true,
          'grok': true,
          'devin': true,
          'fx': true,
        }),
      );
      expect(enabled, agentTypesWithStatusHooks);
    });
  });
}
