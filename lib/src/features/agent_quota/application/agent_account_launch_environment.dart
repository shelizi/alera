import 'package:alera/src/features/settings/domain/alera_settings.dart';

/// Selection affects new PTYs; existing CLI processes own their auth context.
Map<String, String> agentAccountLaunchEnvironment(
  AgentQuotaHostSettings settings,
) {
  return <String, String>{
    if (settings.codexProfiles.any(
      (entry) => entry.profile == settings.selectedCodexProfile,
    ))
      'ALERA_ACCOUNT_CODEX_HOME': settings.selectedCodexProfile,
    if (settings.claudeProfiles.any(
      (entry) => entry.profile == settings.selectedClaudeProfile,
    ))
      'ALERA_ACCOUNT_CLAUDE_PROFILE': settings.selectedClaudeProfile,
  };
}
