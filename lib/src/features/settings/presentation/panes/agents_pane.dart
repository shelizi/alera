import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/forms/alera_setting_row.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/presentation/panes/alera_agent_profiles_skill_control.dart';
import 'package:alera/src/features/settings/presentation/panes/alera_all_skills_control.dart';
import 'package:alera/src/features/settings/presentation/panes/agents_cli_skill_control.dart';
import 'package:alera/src/features/settings/presentation/panes/alera_orchestration_skill_control.dart';
import 'package:alera/src/features/settings/presentation/rows/settings_rows.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Agent integration preferences: core and extra Alera skills, per-agent
/// status hooks, and agent-driven behavior toggles.
class const AgentsSettingsPane({
  super.key,
  required final AgentSettings agents,
  final Map<String, GlobalKey> groupKeys = const <String, GlobalKey>{},
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(settingsControllerProvider.notifier);
    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        KeyedSubtree(
          key: groupKeys['cliSkill'],
          child: const AleraSettingsGroup(
            title: 'Alera CLI And Skills',
            description:
                'Register the CLI command and install agent instructions.',
            children: <Widget>[
              AleraSettingRow(
                title: 'Alera CLI Command',
                description: 'Register the Alera command on PATH for terminals and agents.',
                controlWidth: 360,
                child: AleraCliRegistrationControl(),
              ),
              AleraSettingRow(
                title: 'All Alera Skills',
                description: 'Install or update CLI and orchestration skills. Reapplies selected status hooks.',
                controlWidth: 360,
                child: AleraAllSkillsControl(),
              ),
              AleraSettingRow(
                title: 'Alera CLI Skill',
                description: 'Install the Codex skill that teaches agents to use the Alera CLI.',
                controlWidth: 360,
                child: AleraCliSkillControl(),
              ),
              AleraSettingRow(
                title: 'Alera Orchestration Skill',
                description: 'Install or update orchestration and reapply selected status hooks.',
                controlWidth: 360,
                child: AleraOrchestrationSkillControl(),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['extraSkills'],
          child: const AleraSettingsGroup(
            title: 'Extra Skills',
            description:
                'Install optional skills for specialized Alera workflows.',
            children: <Widget>[
              AleraSettingRow(
                title: 'Agent Profiles Skill',
                description: 'Research models and design, manage, and validate quota-aware Agent Profiles.',
                controlWidth: 360,
                child: AleraAgentProfilesSkillControl(),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        AleraSettingsGroup(
          title: 'Agent Executables',
          description: 'Override a supported agent CLI executable on this device. Leave a path blank to use the default command from PATH.',
          children: <Widget>[
            for (final agentType in spawnableAgentProfileAdapters)
              SettingsTextRow(
                key: ValueKey<String>('agent-executable-path-${agentType.key}'),
                title: '${agentDisplayName(agentType)} Executable',
                description:
                    'Full path to the ${agentDisplayName(agentType)} executable. Default: ${agentProfileDefaultCommands[agentType] ?? agentType.key}',
                value: agents.executablePathFor(agentType.key) ?? '',
                hintText:
                    agentProfileDefaultCommands[agentType] ?? agentType.key,
                onChanged: (value) =>
                    controller.setAgentExecutablePath(agentType.key, value),
              ),
            SettingsTextRow(
              key: const ValueKey<String>('git-bash-executable-path'),
              title: 'Git Bash Executable',
              description: 'Full path to git-bash.exe used when Windows Terminal is unavailable. Leave blank to auto-detect Git for Windows.',
              value: agents.gitBashExecutablePath ?? '',
              hintText: r'C:\Program Files\Git\git-bash.exe',
              onChanged: controller.setGitBashExecutablePath,
            ),
          ],
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['hooks'],
          child: AleraSettingsGroup(
            title: 'Status Hooks',
            description: 'Managed hooks let terminal tabs show agent state.',
            children: <Widget>[
              SettingsSwitchRow(
                title: 'Codex Hooks',
                description: 'Use an Alera-managed Codex runtime home with status hooks.',
                value: agents.agentStatusHooks.isEnabled('codex'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.codex, value),
              ),
              SettingsSwitchRow(
                title: 'Claude Code Hooks',
                description: 'Use an Alera-managed Claude Code config with status hooks.',
                value: agents.agentStatusHooks.isEnabled('claude'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.claude, value),
              ),
              SettingsSwitchRow(
                title: 'GitHub Copilot Hooks',
                description:
                    'Use an Alera-managed GitHub Copilot home overlay.',
                value: agents.agentStatusHooks.isEnabled('copilot'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.copilot, value),
              ),
              SettingsSwitchRow(
                title: 'Cursor Hooks',
                description:
                    'Use an Alera-managed Cursor agent plugin wrapper.',
                value: agents.agentStatusHooks.isEnabled('cursor'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.cursor, value),
              ),
              SettingsSwitchRow(
                title: 'Antigravity Hooks',
                description: 'Install Alera-managed Antigravity hooks for the agy CLI. Disable to remove only Alera-managed hook entries.',
                value: agents.agentStatusHooks.isEnabled('agy'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.agy, value),
              ),
              SettingsSwitchRow(
                title: 'OpenCode Hooks',
                description: 'Use an Alera-managed OpenCode config overlay with status plugin.',
                value: agents.agentStatusHooks.isEnabled('opencode'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.opencode, value),
              ),
              SettingsSwitchRow(
                title: 'OpenCode 2 Hooks',
                description: 'Use an Alera-managed OpenCode 2 config overlay with the v2 status plugin.',
                value: agents.agentStatusHooks.isEnabled('opencode2'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.opencode2, value),
              ),
              SettingsSwitchRow(
                title: 'Pi Hooks',
                description: 'Use an Alera-managed Pi agent overlay with status extension.',
                value: agents.agentStatusHooks.isEnabled('pi'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.pi, value),
              ),
              SettingsSwitchRow(
                title: 'Amp Hooks',
                description: 'Use an Alera-managed Amp config overlay.',
                value: agents.agentStatusHooks.isEnabled('amp'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.amp, value),
              ),
              SettingsSwitchRow(
                title: 'Grok Build Hooks',
                description: 'Install Alera-managed Grok build hooks in a dedicated global file.',
                value: agents.agentStatusHooks.isEnabled('grok'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.grok, value),
              ),
              SettingsSwitchRow(
                title: 'Devin Hooks',
                description: 'Install Alera-managed Devin lifecycle hooks in the global Devin config.',
                value: agents.agentStatusHooks.isEnabled('devin'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.devin, value),
              ),
              SettingsSwitchRow(
                title: 'fx Status',
                description: 'Receive fx lifecycle state through its built-in local Herdr integration on macOS and Linux.',
                value: agents.agentStatusHooks.isEnabled('fx'),
                onChanged: (value) =>
                    controller.setAgentStatusHookEnabled(.fx, value),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['behavior'],
          child: AleraSettingsGroup(
            title: 'Behavior',
            description: 'How Alera reacts while agents are running.',
            children: <Widget>[
              AleraSettingRow(
                title: 'Built-in Alera MCP',
                description: agents.builtinMcpEnabled == null
                    ? 'Unavailable: connect to an updated Runtime Host.'
                    : 'Register alera-workbench for new Managed Codex and Claude sessions. Existing sessions must be restarted; other providers are not supported.',
                child: Switch(
                  value: agents.builtinMcpEnabled ?? false,
                  onChanged: agents.builtinMcpEnabled == null
                      ? null
                      : controller.setBuiltinMcpEnabled,
                ),
              ),
              const AleraSettingRow(
                title: 'MCP connection diagnostics',
                description: 'Inspect the last checked Host connection and available MCP tools.',
                controlWidth: 360,
                child: _BuiltinMcpDiagnostics(),
              ),
              SettingsSwitchRow(
                title: 'Show Tab Titles in Sidebar',
                description: 'Use each agent tab title under a workspace instead of the latest activity.',
                value: agents.showTabTitlesInSidebar,
                onChanged: (value) =>
                    controller.setShowTabTitlesInSidebar(value),
              ),
              SettingsSwitchRow(
                title: 'Show Agent Task Summaries in Sidebar',
                description: 'Show the overall task goal and latest Agent report in expanded multiline sidebar rows. Off by default; the right Agent Context panel stays available.',
                value: agents.showTaskSummariesInSidebar,
                onChanged: (value) =>
                    controller.setShowTaskSummariesInSidebar(value),
              ),
              SettingsSwitchRow(
                title: 'Agent Status Notifications',
                description: 'Show native notifications when an agent needs attention. Bursts are grouped into one notification.',
                value: agents.agentStatusNotificationsEnabled,
                onChanged: (value) =>
                    controller.setAgentStatusNotificationsEnabled(value),
              ),
              SettingsSwitchRow(
                title: 'Agent Finished Notifications',
                description: 'Also notify when an agent finishes. Most agents report the end of a turn, not the end of a task, so this notifies on every reply.',
                value: agents.agentStatusFinishedNotificationsEnabled,
                onChanged: (value) => controller
                    .setAgentStatusFinishedNotificationsEnabled(value),
              ),
              SettingsSwitchRow(
                title: 'Keep Computer Awake While Agents Are Working',
                description: _agentAwakeSettingDescription(
                  Theme.of(context).platform,
                ),
                value: agents.keepComputerAwakeWhileAgentsWork,
                onChanged: (value) =>
                    controller.setKeepComputerAwakeWhileAgentsWork(value),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Explicit refresh avoids adding another idle Host polling loop.
class _BuiltinMcpDiagnostics extends ConsumerStatefulWidget {
  const _BuiltinMcpDiagnostics();

  @override
  ConsumerState<_BuiltinMcpDiagnostics> createState() =>
      _BuiltinMcpDiagnosticsState();
}

class _BuiltinMcpDiagnosticsState
    extends ConsumerState<_BuiltinMcpDiagnostics> {
  Future<Map<Object?, Object?>>? _snapshot;

  Future<Map<Object?, Object?>> _load() async {
    final response = await ref
        .read(runtimeHostClientProvider)
        .runtimeRequest('desktopSettings.get');
    if (response is Map<Object?, Object?>) {
      final diagnostics = response['builtinMcpDiagnostics'];
      if (diagnostics is Map<Object?, Object?>) return diagnostics;
    }
    throw StateError('MCP diagnostics unavailable');
  }

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<Map<Object?, Object?>>(
      future: _snapshot,
      builder: (context, snapshot) {
        final data = snapshot.data;
        final waiting = snapshot.connectionState == ConnectionState.waiting;
        final sessions = data?['sessions'];
        final tools = data?['tools'];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            OutlinedButton(
              onPressed: waiting
                  ? null
                  : () => setState(() => _snapshot = _load()),
              child: Text(context.tr('Refresh MCP diagnostics')),
            ),
            if (waiting)
              const LinearProgressIndicator()
            else if (snapshot.hasError)
              Text(context.tr('MCP diagnostics unavailable'))
            else if (data != null) ...[
              Text(context.tr('Last Host snapshot')),
              if (sessions is List<Object?>)
                for (final session
                    in sessions.whereType<Map<Object?, Object?>>())
                  Text(
                    '${session['provider'] ?? 'Agent'} · ${session['handle']} · ${session['status']}',
                  ),
              if (sessions is List<Object?> && sessions.isEmpty)
                Text(context.tr('No running Agent sessions')),
              if (tools is List<Object?>)
                ExpansionTile(
                  title: Text(
                    '${context.tr('Available MCP tools')} (${tools.length})',
                  ),
                  children: [
                    for (final tool in tools.whereType<String>())
                      SelectableText(tool),
                  ],
                ),
            ],
          ],
        );
      },
    );
  }
}

String _agentAwakeSettingDescription(TargetPlatform platform) {
  if (platform == TargetPlatform.windows) {
    return 'Keeps this computer and display awake while agents are working. Lid-close behavior follows this device\'s power settings.';
  }
  return 'Keeps this computer and display awake while agents are working. Alera also asks this device to stay awake when the lid is closed, subject to its power policy.';
}
