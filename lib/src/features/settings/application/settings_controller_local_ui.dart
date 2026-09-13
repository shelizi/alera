part of 'settings_controller.dart';

/// Mixin providing operations for the local-only UI prefs tier.
///
/// These preferences are strictly local to this device/window UI and never
/// synchronize with the runtime host daemon or cloud configuration.
mixin _SettingsControllerLocalUiSettings on _$SettingsController {
  SettingsController get _controller => this as SettingsController;

  Future<void> setAppLanguage(AppLanguage value) =>
      _controller._serialize(() async {
        if (state.general.language == value) {
          return;
        }
        await _controller._save(
          state.copyWith(general: state.general.copyWith(language: value)),
        );
      });

  Future<void> markStarClicked() => _controller._serialize(() async {
    if (state.general.starClicked) {
      return;
    }
    await _controller._save(
      state.copyWith(general: state.general.copyWith(starClicked: true)),
    );
  });

  Future<void> setKeepAliveEnabled(bool value) =>
      _controller._serialize(() async {
        if (state.general.keepAliveEnabled == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(keepAliveEnabled: value),
          ),
        );
      });

  Future<void> setDiagnosticsLogLevel(DiagnosticsLogLevel value) =>
      _controller._serialize(() async {
        if (state.diagnostics.logLevel == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            diagnostics: state.diagnostics.copyWith(logLevel: value),
          ),
        );
      });

  Future<void> setCrashReportingEnabled(bool value) => _controller._serialize(
    () async {
      if (state.diagnostics.crashReportingEnabled == value) {
        return;
      }
      await _controller._save(
        state.copyWith(
          diagnostics: state.diagnostics.copyWith(crashReportingEnabled: value),
        ),
      );
    },
  );

  Future<void> setKeepComputerAwakeWhileAgentsWork(bool value) =>
      _controller._serialize(() async {
        if (state.agents.keepComputerAwakeWhileAgentsWork == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(
              keepComputerAwakeWhileAgentsWork: value,
            ),
          ),
        );
      });

  Future<void> setShowPullRequestStatusInSidebar(bool value) =>
      _controller._serialize(() async {
        if (state.general.showPullRequestStatusInSidebar == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(
              showPullRequestStatusInSidebar: value,
            ),
          ),
        );
      });

  Future<void> setPullRequestFailureNotificationsEnabled(bool value) =>
      _controller._serialize(() async {
        if (state.general.pullRequestFailureNotificationsEnabled == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            general: state.general.copyWith(
              pullRequestFailureNotificationsEnabled: value,
            ),
          ),
        );
      });

  Future<void> setClaudeQuotaProfiles({
    required String hostId,
    required List<ClaudeQuotaProfileSettings> profiles,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    final selected =
        current.selectedClaudeProfile == 'default' ||
            profiles.any(
              (profile) => profile.profile == current.selectedClaudeProfile,
            )
        ? current.selectedClaudeProfile
        : 'default';
    final validClaudeKeys = <String>{
      AgentQuotaHostSettings.quotaPinKey(AgentQuotaProviderId.claude),
      for (final profile in profiles)
        AgentQuotaHostSettings.quotaPinKey(
          AgentQuotaProviderId.claude,
          claudeAccountId: profile.profile,
        ),
    };
    final unpinned = current.unpinnedQuotaKeys
        .where(
          (key) => !key.startsWith('claude:') || validClaudeKeys.contains(key),
        )
        .toList();
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(
        claudeProfiles: profiles,
        selectedClaudeProfile: selected,
        unpinnedQuotaKeys: unpinned,
      ),
    );
  });

  Future<void> setSelectedClaudeQuotaProfile({
    required String hostId,
    required String profile,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(selectedClaudeProfile: profile),
    );
  });

  Future<void> setAgentQuotaPinned({
    required String hostId,
    required String pinKey,
    required bool pinned,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    final unpinned = <String>{...current.unpinnedQuotaKeys};
    if (pinned) {
      unpinned.remove(pinKey);
    } else {
      unpinned.add(pinKey);
    }
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(unpinnedQuotaKeys: unpinned.toList()),
    );
  });
}
