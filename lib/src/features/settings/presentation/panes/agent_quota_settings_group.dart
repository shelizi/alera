import 'package:alera/src/app/localization/alera_localizations.dart';

import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/forms/alera_checkbox.dart';
import 'package:alera/src/design_system/forms/alera_setting_row.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/agent_quota/presentation/agent_quota_provider_icon.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/presentation/rows/settings_rows.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'codex_quota_profiles_control.dart';
import 'agy_quota_profiles_control.dart';

part 'agent_quota_settings_controls.dart';

class const AgentQuotaSettingsPane({
  super.key,
  required final AgentQuotaSettings settings,
  final Map<String, GlobalKey> groupKeys = const <String, GlobalKey>{},
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final hostId = ref.watch(
      workbenchControllerProvider.select(
        (state) => state.activeWorkspace?.hostId ?? 'local',
      ),
    );
    final hostSettings = settings.forHost(hostId);
    final quotaState = ref.watch(agentQuotaStateProvider);
    final environment = quotaState.value?.hostId == hostId
        ? quotaState.value?.environment ?? const <String, bool>{}
        : const <String, bool>{};
    final controller = ref.read(settingsControllerProvider.notifier);

    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        KeyedSubtree(
          key: groupKeys['providers'],
          child: AleraSettingsGroup(
            title: 'Provider Quotas',
            description: 'Choose which usage sources appear for the active workspace host.',
            children: <Widget>[
              AleraSettingRow(
                title: 'Active Quota Host',
                description: 'Run quota commands locally or through the installed Alera runtime for this workspace.',
                child: Align(
                  alignment: Alignment.centerRight,
                  child: Text(
                    hostId == 'local' ? 'Local' : hostId,
                    overflow: .ellipsis,
                    style: AleraTokens.monoStyle,
                  ),
                ),
              ),
              for (final provider in AgentQuotaProviderId.values)
                if (provider != AgentQuotaProviderId.claude)
                  SettingsSwitchRow(
                    title: '${provider.label} Quotas',
                    description: _providerDescription(provider),
                    value: hostSettings.enabledProviders.contains(provider),
                    secondary: _QuotaPinButton(
                      pinned: hostSettings.isQuotaPinned(provider),
                      enabled: hostSettings.enabledProviders.contains(provider),
                      onChanged: (pinned) {
                        unawaited(
                          controller.setAgentQuotaPinned(
                            hostId: hostId,
                            pinKey: AgentQuotaHostSettings.quotaPinKey(
                              provider,
                            ),
                            pinned: pinned,
                          ),
                        );
                      },
                    ),
                    onChanged: (value) {
                      unawaited(
                        controller.setAgentQuotaProviderEnabled(
                          hostId: hostId,
                          provider: provider,
                          value: value,
                        ),
                      );
                    },
                  ),
              AleraSettingRow(
                title: 'Quota Display Order',
                description: 'Set the left-to-right order of enabled providers in the status bar.',
                controlWidth: 420,
                child: _ProviderOrderControl(
                  providers: hostSettings.enabledProviders,
                  onChanged: (providers) {
                    unawaited(
                      controller.setAgentQuotaProviderOrder(
                        hostId: hostId,
                        providers: providers,
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['claude'],
          child: AleraSettingsGroup(
            title: 'Claude',
            description: 'Configure the default Claude account, CCS profiles, and independent account directories.',
            children: <Widget>[
              SettingsSwitchRow(
                title: 'Claude Code Quotas',
                description: _providerDescription(.claude),
                value: hostSettings.enabledProviders.contains(
                  AgentQuotaProviderId.claude,
                ),
                onChanged: (value) {
                  unawaited(
                    controller.setAgentQuotaProviderEnabled(
                      hostId: hostId,
                      provider: .claude,
                      value: value,
                    ),
                  );
                },
              ),
              SettingsSwitchRow(
                title: 'Claude Default Quotas',
                description: 'Query the default Claude account separately from configured CCS profiles.',
                value: hostSettings.claudeDefaultEnabled,
                secondary: _QuotaPinButton(
                  pinned: hostSettings.isQuotaPinned(.claude),
                  enabled:
                      hostSettings.claudeDefaultEnabled &&
                      hostSettings.enabledProviders.contains(
                        AgentQuotaProviderId.claude,
                      ),
                  onChanged: (pinned) {
                    unawaited(
                      controller.setAgentQuotaPinned(
                        hostId: hostId,
                        pinKey: AgentQuotaHostSettings.quotaPinKey(.claude),
                        pinned: pinned,
                      ),
                    );
                  },
                ),
                onChanged: (value) {
                  unawaited(
                    controller.setClaudeDefaultQuotaEnabled(
                      hostId: hostId,
                      value: value,
                    ),
                  );
                },
              ),
              SettingsSwitchRow(
                title: 'Claude Default in Usage',
                description: 'Include the default Claude account in Usage independently of quota polling.',
                value: hostSettings.claudeDefaultShowInUsage,
                onChanged: (value) {
                  unawaited(
                    controller.setClaudeDefaultShowInUsage(
                      hostId: hostId,
                      value: value,
                    ),
                  );
                },
              ),
              AleraSettingRow(
                title: 'Claude Accounts',
                description: 'Add a CCS instance name or an absolute CLAUDE_CONFIG_DIR. Switch accounts from the quota card.',
                controlWidth: 420,
                child: _ClaudeProfilesControl(
                  profiles: hostSettings.claudeProfiles,
                  isPinned: (profile) => hostSettings.isQuotaPinned(
                    .claude,
                    claudeAccountId: profile,
                  ),
                  onPinnedChanged: (profile, pinned) {
                    unawaited(
                      controller.setAgentQuotaPinned(
                        hostId: hostId,
                        pinKey: AgentQuotaHostSettings.quotaPinKey(
                          .claude,
                          claudeAccountId: profile,
                        ),
                        pinned: pinned,
                      ),
                    );
                  },
                  onChanged: (profiles) {
                    unawaited(
                      controller.setClaudeQuotaProfiles(
                        hostId: hostId,
                        profiles: profiles,
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        AleraSettingsGroup(
          title: 'Codex Accounts',
          description: 'Add signed-in CODEX_HOME directories. Choose the active account from its quota card.',
          children: <Widget>[
            AleraSettingRow(
              title: 'Codex Accounts',
              description: 'Each directory keeps its own CLI login, configuration, and history. The default account is retained.',
              controlWidth: 420,
              child: CodexQuotaProfilesControl(
                profiles: hostSettings.codexProfiles,
                onChanged: (profiles) => unawaited(
                  controller.setCodexQuotaProfiles(
                    hostId: hostId,
                    profiles: profiles,
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: AleraTokens.space16),
        AleraSettingsGroup(
          title: 'Google / Antigravity Accounts',
          description: 'Register additional account labels. Independent Google login requires a verified OS-isolated credential backend; registration alone cannot switch accounts.',
          children: <Widget>[
            AleraSettingRow(
              title: 'Additional Google Accounts',
              description: 'Registered accounts appear as unavailable until verified. No Google credentials are copied or changed.',
              controlWidth: 420,
              child: AgyQuotaProfilesControl(
                profiles: hostSettings.agyProfiles,
                selectedAccountId: hostSettings.selectedAgyAccountId,
                onSelected: (accountId) => unawaited(
                  controller.setSelectedAgyAccount(
                    hostId: hostId,
                    accountId: accountId,
                  ),
                ),
                onChanged: (profiles) => unawaited(
                  controller.setAgyQuotaProfiles(
                    hostId: hostId,
                    profiles: profiles,
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['historySync'],
          child: AleraSettingsGroup(
            title: 'Agent History Sync',
            description: 'Keep account logins separate. Share native conversation history only between accounts of the same Agent on this host.',
            children: <Widget>[
              AleraSettingRow(
                title: 'History Sharing Mode',
                description: 'Accounts remain separate by default. Choose Sync All or Sync by Group to explicitly share history with other accounts of the same Agent.',
                controlWidth: 340,
                child: DropdownButton<AgentHistorySyncMode>(
                  isExpanded: true,
                  value: hostSettings.historySync.mode,
                  items: const <DropdownMenuItem<AgentHistorySyncMode>>[
                    DropdownMenuItem(
                      value: AgentHistorySyncMode.separate,
                      child: Text('Keep Separate (Default)'),
                    ),
                    DropdownMenuItem(
                      value: AgentHistorySyncMode.all,
                      child: Text('Sync All (Opt-in)'),
                    ),
                    DropdownMenuItem(
                      value: AgentHistorySyncMode.groups,
                      child: Text('Sync by Group (Opt-in)'),
                    ),
                  ],
                  onChanged: (mode) {
                    if (mode == null) return;
                    unawaited(
                      controller.setAgentHistorySyncMode(
                        hostId: hostId,
                        mode: mode,
                      ),
                    );
                  },
                ),
              ),
              if (hostSettings.historySync.mode ==
                  AgentHistorySyncMode.groups) ...[
                _historySyncGroupRow(
                  hostId: hostId,
                  provider: 'codex',
                  accountId: 'default',
                  label: 'Codex Default',
                  sync: hostSettings.historySync,
                  controller: controller,
                ),
                for (final profile in hostSettings.codexProfiles)
                  _historySyncGroupRow(
                    hostId: hostId,
                    provider: 'codex',
                    accountId: profile.profile,
                    label: 'Codex · ${profile.alias}',
                    sync: hostSettings.historySync,
                    controller: controller,
                  ),
                _historySyncGroupRow(
                  hostId: hostId,
                  provider: 'claude',
                  accountId: 'default',
                  label: 'Claude Default',
                  sync: hostSettings.historySync,
                  controller: controller,
                ),
                for (final profile in hostSettings.claudeProfiles)
                  _historySyncGroupRow(
                    hostId: hostId,
                    provider: 'claude',
                    accountId: profile.profile,
                    label: 'Claude · ${profile.alias}',
                    sync: hostSettings.historySync,
                    controller: controller,
                  ),
                _historySyncGroupRow(
                  hostId: hostId,
                  provider: 'agy',
                  accountId: 'default',
                  label: 'Antigravity Default',
                  sync: hostSettings.historySync,
                  controller: controller,
                ),
                for (final profile in hostSettings.agyProfiles)
                  _historySyncGroupRow(
                    hostId: hostId,
                    provider: 'agy',
                    accountId: profile.accountId,
                    label: 'Antigravity · ${profile.alias} (not supported)',
                    sync: hostSettings.historySync,
                    controller: controller,
                  ),
                const Padding(
                  padding: EdgeInsets.only(top: AleraTokens.space8),
                  child: Text(
                    'Enter the same group name for accounts that should sync. '
                    'An empty group remains isolated. Group names do not mix Codex, Claude or AGY formats. '
                    'Antigravity history mirroring is not supported yet.',
                  ),
                ),
              ],
              const Padding(
                padding: EdgeInsets.only(top: AleraTokens.space8),
                child: Text(
                  'History only: credentials, configuration, active session files, '
                  'and authentication tokens are never shared. Changing to Separate '
                  'does not delete history already copied.',
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: AleraTokens.space16),
        KeyedSubtree(
          key: groupKeys['credentials'],
          child: AleraSettingsGroup(
            title: 'Credential Environment',
            description: 'Configure environment variable names for the active workspace host.',
            children: <Widget>[
              SettingsTextRow(
                key: const ValueKey<String>('kimi-api-key-variable'),
                title: 'Kimi API Key Variable',
                description: 'Environment variable read on the active host. The secret value is never stored by Alera.',
                value: hostSettings.environment.kimiApiKey,
                hintText: 'KIMI_API_KEY',
                onChanged: (value) => _saveEnvironment(
                  controller,
                  hostId,
                  hostSettings.environment.copyWith(kimiApiKey: value),
                ),
              ),
              SettingsTextRow(
                title: 'Z.ai API Key Variable',
                description: 'Environment variable read on the active host. The secret value is never stored by Alera.',
                value: hostSettings.environment.zaiApiKey,
                hintText: 'ZAI_API_KEY',
                onChanged: (value) => _saveEnvironment(
                  controller,
                  hostId,
                  hostSettings.environment.copyWith(zaiApiKey: value),
                ),
              ),
              SettingsTextRow(
                title: 'Z.ai Base URL Variable',
                description: 'Optional environment variable for the coding plan API base URL.',
                value: hostSettings.environment.zaiBaseUrl,
                hintText: 'ZAI_BASE_URL',
                onChanged: (value) => _saveEnvironment(
                  controller,
                  hostId,
                  hostSettings.environment.copyWith(zaiBaseUrl: value),
                ),
              ),
              SettingsTextRow(
                title: 'MiniMax API Key Variable',
                description: 'Environment variable read on the active host. The secret value is never stored by Alera.',
                value: hostSettings.environment.minimaxApiKey,
                hintText: 'MINIMAX_API_KEY',
                onChanged: (value) => _saveEnvironment(
                  controller,
                  hostId,
                  hostSettings.environment.copyWith(minimaxApiKey: value),
                ),
              ),
              SettingsTextRow(
                title: 'MiniMax API Host Variable',
                description: 'Optional environment variable selecting the global or china token plan endpoint.',
                value: hostSettings.environment.minimaxApiHost,
                hintText: 'MINIMAX_API_HOST',
                onChanged: (value) => _saveEnvironment(
                  controller,
                  hostId,
                  hostSettings.environment.copyWith(minimaxApiHost: value),
                ),
              ),
              AleraSettingRow(
                title: 'Credential Availability',
                description: 'Check whether each configured variable exists without reading its secret value.',
                controlWidth: 420,
                child: _EnvironmentPresence(
                  names: <String>[
                    hostSettings.environment.kimiApiKey,
                    hostSettings.environment.zaiApiKey,
                    hostSettings.environment.zaiBaseUrl,
                    hostSettings.environment.minimaxApiKey,
                    hostSettings.environment.minimaxApiHost,
                  ],
                  presence: environment,
                  loading: quotaState.isLoading,
                  onRefresh: () {
                    ref
                        .read(agentQuotaServiceProvider)
                        .requestForceRefresh(hostId);
                    ref.invalidate(agentQuotaStateProvider);
                  },
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

Widget _historySyncGroupRow({
  required String hostId,
  required String provider,
  required String accountId,
  required String label,
  required AgentHistorySyncSettings sync,
  required SettingsController controller,
}) {
  return SettingsTextRow(
    key: ValueKey<String>('history-sync-group-$provider:$accountId'),
    title: label,
    description: accountId,
    value: sync.accountGroups['$provider:$accountId'] ?? '',
    hintText: 'e.g. work',
    onChanged: (value) {
      unawaited(
        controller.setAgentHistorySyncAccountGroup(
          hostId: hostId,
          provider: provider,
          accountId: accountId,
          group: value,
        ),
      );
    },
  );
}

void _saveEnvironment(
  SettingsController controller,
  String hostId,
  AgentQuotaEnvironmentSettings environment,
) {
  unawaited(
    controller.setAgentQuotaEnvironment(
      hostId: hostId,
      environment: environment,
    ),
  );
}

String _providerDescription(AgentQuotaProviderId provider) {
  return switch (provider) {
    AgentQuotaProviderId.claude =>
      'Read default Claude Code usage and any configured CCS profiles.',
    AgentQuotaProviderId.codex =>
      'Read Codex rate limits through the official app server.',
    AgentQuotaProviderId.kimi =>
      'Read Kimi Coding Plan usage with an API key from the host environment.',
    AgentQuotaProviderId.grok =>
      'Read Grok Build usage through its official interactive CLI.',
    AgentQuotaProviderId.cursor =>
      'Read Cursor plan usage from the local Cursor CLI session.',
    AgentQuotaProviderId.antigravity =>
      'Read Antigravity usage through the official agy CLI.',
    AgentQuotaProviderId.minimax => 'Read MiniMax Token Plan usage with an API key from the host environment.',
    AgentQuotaProviderId.zai =>
      'Read Z.ai limits with an API key from the host environment.',
    AgentQuotaProviderId.devin => "Read Devin daily and weekly quota from the host's signed-in Devin CLI session.",
    AgentQuotaProviderId.opencode => 'Estimate OpenCode Go quota and local OpenCode Zen spend from the host database.',
  };
}

class const _EnvironmentPresence({
  required final List<String> names,
  required final Map<String, bool> presence,
  required final bool loading,
  required final VoidCallback onRefresh,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        for (final name in names)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: AleraTokens.space2),
            child: Row(
              children: <Widget>[
                Icon(
                  presence[name] == true ? AleraIcons.check : AleraIcons.close,
                  size: 13,
                  color: presence[name] == true
                      ? AleraTokens.success
                      : AleraTokens.foregroundFaint,
                ),
                const SizedBox(width: AleraTokens.space6),
                Expanded(
                  child: Text(
                    name,
                    overflow: .ellipsis,
                    style: AleraTokens.monoStyle,
                  ),
                ),
              ],
            ),
          ),
        const SizedBox(height: AleraTokens.space6),
        Align(
          alignment: Alignment.centerRight,
          child: TextButton.icon(
            onPressed: loading ? null : onRefresh,
            icon: Icon(
              loading ? AleraIcons.loading : AleraIcons.refresh,
              size: 14,
            ),
            label: Text(context.tr('Check Environment')),
          ),
        ),
      ],
    );
  }
}
