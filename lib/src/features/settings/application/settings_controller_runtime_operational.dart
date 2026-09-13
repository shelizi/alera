part of 'settings_controller.dart';

/// Mixin providing operations for the runtime operational settings tier.
///
/// These settings govern the terminal-host sidecar daemon (`ServerActor` /
/// `runtimeMetadata`) and background processes.
mixin _SettingsControllerRuntimeOperationalSettings on _$SettingsController {
  SettingsController get _controller => this as SettingsController;

  Future<void> updateWorkspaceDirectory(String? path) =>
      _controller._serialize(() async {
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(workspaceDirectory: path),
          ),
        );
      });

  Future<void> setConfirmProjectRemoval(bool value) =>
      _controller._serialize(() async {
        if (state.general.confirmProjectRemoval == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(confirmProjectRemoval: value),
          ),
        );
      });

  Future<void> setConfirmWorkspaceRemoval(bool value) =>
      _controller._serialize(() async {
        if (state.general.confirmWorkspaceRemoval == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(confirmWorkspaceRemoval: value),
          ),
        );
      });

  /// `0` disables the automatic archive sweep entirely.
  Future<void> setAutoArchiveWorkspacesAfterDays(int value) =>
      _controller._serialize(() async {
        final next = value < 0 ? 0 : value;
        if (state.general.autoArchiveWorkspacesAfterDays == next) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(
              autoArchiveWorkspacesAfterDays: next,
            ),
          ),
        );
      });

  Future<void> setAgentStatusHookEnabled(
    AgentType agentType,
    bool value,
  ) => _controller._serialize(() async {
    final current = state.agents.agentStatusHooks;
    final next = <String, Object?>{...current.toMap(), agentType.key: value};
    final updated = AgentStatusHookSettings.fromJson(next);
    if (current == updated) {
      return;
    }
    await _controller._save(
      state.copyWith(agents: state.agents.copyWith(agentStatusHooks: updated)),
    );
  });

  Future<void> setDefaultAgentProfile(String? profileId) =>
      _controller._serialize(() async {
        final normalized = profileId?.trim();
        final next = normalized == null || normalized.isEmpty
            ? null
            : normalized;
        if (state.agents.defaultAgentProfileId == next) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(defaultAgentProfileId: next),
          ),
        );
      });

  Future<void> updateAiAssist(
    AiAssistSettings Function(AiAssistSettings) edit,
  ) => _controller._serialize(() async {
    await _controller._save(state.copyWith(aiAssist: edit(state.aiAssist)));
  });

  Future<void> resetAiAssistSettings() => _controller._serialize(() async {
    await _controller._save(state.copyWith(aiAssist: .defaults));
  });

  Future<void> updateTextActions(
    TextActionsSettings Function(TextActionsSettings) edit,
  ) => _controller._serialize(() async {
    await _controller._save(
      state.copyWith(textActions: edit(state.textActions)),
    );
  });

  Future<void> resetTextActions() => _controller._serialize(() async {
    await _controller._save(state.copyWith(textActions: .defaults));
  });

  Future<void> setAgentQuotaProviderEnabled({
    required String hostId,
    required AgentQuotaProviderId provider,
    required bool value,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    final enabled = <AgentQuotaProviderId>{...current.enabledProviders};
    if (value) {
      enabled.add(provider);
    } else {
      enabled.remove(provider);
    }
    await _saveQuotaHost(
      hostId,
      current.copyWith(enabledProviders: enabled.toList()),
    );
  });

  Future<void> setAgentQuotaProviderOrder({
    required String hostId,
    required List<AgentQuotaProviderId> providers,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    final enabled = current.enabledProviders.toSet();
    final ordered = <AgentQuotaProviderId>[
      for (final provider in providers)
        if (enabled.remove(provider)) provider,
      for (final provider in current.enabledProviders)
        if (enabled.remove(provider)) provider,
    ];
    await _saveQuotaHost(hostId, current.copyWith(enabledProviders: ordered));
  });

  Future<void> setClaudeDefaultQuotaEnabled({
    required String hostId,
    required bool value,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    await _saveQuotaHost(hostId, current.copyWith(claudeDefaultEnabled: value));
  });

  Future<void> setClaudeDefaultShowInUsage({
    required String hostId,
    required bool value,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    await _saveQuotaHost(
      hostId,
      current.copyWith(claudeDefaultShowInUsage: value),
    );
  });

  Future<void> setAgentQuotaEnvironment({
    required String hostId,
    required AgentQuotaEnvironmentSettings environment,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    await _saveQuotaHost(hostId, current.copyWith(environment: environment));
  });

  Future<void> _saveQuotaHost(
    String hostId,
    AgentQuotaHostSettings hostSettings,
  ) async {
    final quotas = state.agents.quotas.withHost(hostId, hostSettings);
    await _controller._save(
      state.copyWith(agents: state.agents.copyWith(quotas: quotas)),
    );
  }
}
