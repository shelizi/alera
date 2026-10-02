import 'package:alera/src/features/agent_quota/application/agent_account_launch_environment.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('default selection preserves the native CLI environment', () {
    expect(
      agentAccountLaunchEnvironment(AgentQuotaHostSettings.defaults),
      isEmpty,
    );
  });

  test('both accounts are isolated for future launches on each host', () {
    for (final directory in [
      '/home/user/codex-work',
      r'C:\Users\user\codex-work',
    ]) {
      final settings = AgentQuotaHostSettings(
        codexProfiles: [
          CodexQuotaProfileSettings(alias: 'Work', profile: directory),
        ],
        selectedCodexProfile: directory,
        claudeProfiles: const [
          ClaudeQuotaProfileSettings(alias: 'Work', profile: 'work'),
        ],
        selectedClaudeProfile: 'work',
      );
      final existingLaunch = agentAccountLaunchEnvironment(settings);
      final futureLaunch = agentAccountLaunchEnvironment(
        settings.copyWith(
          selectedCodexProfile: 'default',
          selectedClaudeProfile: 'default',
        ),
      );
      expect(existingLaunch, {
        'ALERA_ACCOUNT_CODEX_HOME': directory,
        'ALERA_ACCOUNT_CLAUDE_PROFILE': 'work',
      });
      expect(futureLaunch, isEmpty);
      expect(existingLaunch['ALERA_ACCOUNT_CODEX_HOME'], directory);
    }
  });

  test('removed accounts fall back without leaking stale selection', () {
    expect(
      agentAccountLaunchEnvironment(
        const AgentQuotaHostSettings(
          selectedCodexProfile: '/deleted',
          selectedClaudeProfile: 'deleted',
        ),
      ),
      isEmpty,
    );
  });

  test('account configuration and selection survive settings round trips', () {
    final settings = AgentQuotaHostSettings.fromJson({
      'codexProfiles': [
        {'alias': 'Work', 'profile': '/codex-work'},
      ],
      'selectedCodexProfile': '/codex-work',
    });
    expect(settings.codexProfiles.single.alias, 'Work');
    expect(
      AgentQuotaHostSettings.fromJson(settings.toMap()).selectedCodexProfile,
      '/codex-work',
    );
    expect(
      AgentQuotaHostSettings.quotaPinKey(
        .codex,
        claudeAccountId: '/codex-work',
      ),
      'codex:/codex-work',
    );
    expect(AgentQuotaHostSettings.quotaPinKey(.codex), 'codex');
  });
}
