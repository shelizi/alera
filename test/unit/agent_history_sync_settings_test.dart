import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('legacy settings default to same-provider all-account sync', () {
    final host = AgentQuotaHostSettings.fromJson(<String, Object?>{});
    expect(host.historySync.mode, AgentHistorySyncMode.all);
    expect(host.historySync.shouldSync('codex', 'default', 'work'), isTrue);
    expect(host.historySync.shouldSync('codex', 'default', 'default'), isFalse);
    expect(host.historySync.shouldSync('grok', 'default', 'work'), isFalse);
  });

  test('separate mode never connects different accounts', () {
    const settings = AgentHistorySyncSettings(
      mode: AgentHistorySyncMode.separate,
    );
    expect(settings.shouldSync('codex', 'default', 'work'), isFalse);
    expect(settings.shouldSync('claude', 'default', 'work'), isFalse);
    expect(settings.shouldSync('agy', 'default', 'work'), isFalse);
  });

  test('groups require same provider and matching nonempty name', () {
    const settings = AgentHistorySyncSettings(
      mode: AgentHistorySyncMode.groups,
      accountGroups: <String, String>{
        'codex:default': 'work',
        'codex:one': 'work',
        'codex:personal': 'personal',
        'claude:default': 'work',
        'claude:one': 'work',
      },
    );
    expect(settings.shouldSync('codex', 'default', 'one'), isTrue);
    expect(settings.shouldSync('codex', 'default', 'personal'), isFalse);
    expect(settings.shouldSync('codex', 'default', 'missing'), isFalse);
    expect(settings.shouldSync('claude', 'default', 'one'), isTrue);
    expect(settings.shouldSync('agy', 'default', 'one'), isFalse);
  });

  test('history sync persists in quota settings roundtrip', () {
    const settings = AgentQuotaHostSettings(
      historySync: AgentHistorySyncSettings(
        mode: AgentHistorySyncMode.groups,
        accountGroups: <String, String>{
          'codex:default': 'team',
          'codex:one': 'team',
        },
      ),
    );
    final restored = AgentQuotaHostSettings.fromJson(settings.toMap());
    expect(restored.historySync.mode, AgentHistorySyncMode.groups);
    expect(restored.historySync.shouldSync('codex', 'default', 'one'), isTrue);
  });
}
