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

  Future<void> setAgentExecutablePath(String agentKey, String? path) =>
      _controller._serialize(() async {
        final normalizedKey = agentKey.trim().toLowerCase();
        final normalizedPath = path?.trim();
        final next = Map<String, String>.of(state.agents.agentExecutablePaths);
        if (normalizedPath == null || normalizedPath.isEmpty) {
          next.remove(normalizedKey);
        } else {
          next[normalizedKey] = normalizedPath;
        }
        if (mapEquals(next, state.agents.agentExecutablePaths)) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(agentExecutablePaths: next),
          ),
        );
      });

  Future<void> setGitBashExecutablePath(String? path) =>
      _controller._serialize(() async {
        final normalizedPath = path?.trim();
        final next = normalizedPath == null || normalizedPath.isEmpty
            ? null
            : normalizedPath;
        if (state.agents.gitBashExecutablePath == next) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(gitBashExecutablePath: next),
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
    if (profile != 'default' &&
        !current.claudeProfiles.any((entry) => entry.profile == profile)) {
      throw ArgumentError.value(profile, 'profile', 'Unknown Claude account');
    }
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(selectedClaudeProfile: profile),
    );
  });

  Future<void> setCodexQuotaProfiles({
    required String hostId,
    required List<CodexQuotaProfileSettings> profiles,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    final selected =
        profiles.any(
          (profile) => profile.profile == current.selectedCodexProfile,
        )
        ? current.selectedCodexProfile
        : 'default';
    final validKeys = <String>{
      for (final profile in profiles) 'codex:${profile.profile}',
    };
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(
        codexProfiles: profiles,
        selectedCodexProfile: selected,
        unpinnedQuotaKeys: current.unpinnedQuotaKeys
            .where(
              (key) => !key.startsWith('codex:') || validKeys.contains(key),
            )
            .toList(),
      ),
    );
  });

  Future<void> setSelectedCodexQuotaProfile({
    required String hostId,
    required String profile,
  }) => _controller._serialize(() async {
    final current = state.agents.quotas.forHost(hostId);
    if (profile != 'default' &&
        !current.codexProfiles.any((entry) => entry.profile == profile)) {
      throw ArgumentError.value(profile, 'profile', 'Unknown Codex account');
    }
    await _controller._saveQuotaHost(
      hostId,
      current.copyWith(selectedCodexProfile: profile),
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
