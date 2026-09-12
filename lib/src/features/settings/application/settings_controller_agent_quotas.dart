part of 'settings_controller.dart';

mixin _SettingsControllerAgentQuotaSettings on _$SettingsController {
  SettingsController get _agentQuotaSettingsController =>
      this as SettingsController;

  Future<void> setAgentQuotaProviderEnabled({
    required String hostId,
    required AgentQuotaProviderId provider,
    required bool value,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
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
  }

  Future<void> setAgentQuotaProviderOrder({
    required String hostId,
    required List<AgentQuotaProviderId> providers,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
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
  }

  Future<void> setClaudeQuotaProfiles({
    required String hostId,
    required List<ClaudeQuotaProfileSettings> profiles,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      final selected =
          current.selectedClaudeProfile == 'default' ||
              profiles.any(
                (profile) => profile.profile == current.selectedClaudeProfile,
              )
          ? current.selectedClaudeProfile
          : 'default';
      final validClaudeKeys = <String>{
        AgentQuotaHostSettings.quotaPinKey(.claude),
        for (final profile in profiles)
          AgentQuotaHostSettings.quotaPinKey(
            .claude,
            claudeAccountId: profile.profile,
          ),
      };
      final unpinned = current.unpinnedQuotaKeys
          .where(
            (key) =>
                !key.startsWith('claude:') || validClaudeKeys.contains(key),
          )
          .toList();
      await _saveQuotaHost(
        hostId,
        current.copyWith(
          claudeProfiles: profiles,
          selectedClaudeProfile: selected,
          unpinnedQuotaKeys: unpinned,
        ),
      );
    });
  }

  Future<void> setAgentQuotaPinned({
    required String hostId,
    required String pinKey,
    required bool pinned,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      final unpinned = <String>{...current.unpinnedQuotaKeys};
      if (pinned) {
        unpinned.remove(pinKey);
      } else {
        unpinned.add(pinKey);
      }
      await _saveQuotaHost(
        hostId,
        current.copyWith(unpinnedQuotaKeys: unpinned.toList()),
      );
    });
  }

  Future<void> setClaudeDefaultQuotaEnabled({
    required String hostId,
    required bool value,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      await _saveQuotaHost(
        hostId,
        current.copyWith(claudeDefaultEnabled: value),
      );
    });
  }

  Future<void> setClaudeDefaultShowInUsage({
    required String hostId,
    required bool value,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      await _saveQuotaHost(
        hostId,
        current.copyWith(claudeDefaultShowInUsage: value),
      );
    });
  }

  Future<void> setSelectedClaudeQuotaProfile({
    required String hostId,
    required String profile,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      await _saveQuotaHost(
        hostId,
        current.copyWith(selectedClaudeProfile: profile),
      );
    });
  }

  Future<void> setAgentQuotaEnvironment({
    required String hostId,
    required AgentQuotaEnvironmentSettings environment,
  }) {
    final controller = _agentQuotaSettingsController;
    return controller._serialize(() async {
      final current = state.agents.quotas.forHost(hostId);
      await _saveQuotaHost(hostId, current.copyWith(environment: environment));
    });
  }

  Future<void> _saveQuotaHost(
    String hostId,
    AgentQuotaHostSettings hostSettings,
  ) async {
    final quotas = state.agents.quotas.withHost(hostId, hostSettings);
    await _agentQuotaSettingsController._save(
      state.copyWith(agents: state.agents.copyWith(quotas: quotas)),
    );
  }
}
