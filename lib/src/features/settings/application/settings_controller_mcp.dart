part of 'settings_controller.dart';

/// Narrow MCP settings projection. Do not serialize AleraSettings as a whole:
/// it may contain executable paths, tokens and private integrations. Paths
/// added here must also be reviewed in the Rust Host admission whitelist.
Object mcpSettingValue(AleraSettings settings, String path) => switch (path) {
  'general.autoGitFetchEnabled' => settings.general.autoGitFetchEnabled,
  'general.autoGitFetchIntervalMinutes' =>
    settings.general.autoGitFetchIntervalMinutes,
  'terminal.fontSize' => settings.terminal.fontSize,
  'editor.autosaveEnabled' => settings.editor.autosaveEnabled,
  'editor.tabSize' => settings.editor.tabSize,
  'general.confirmProjectRemoval' => settings.general.confirmProjectRemoval,
  'general.confirmWorkspaceRemoval' => settings.general.confirmWorkspaceRemoval,
  'general.autoArchiveWorkspacesAfterDays' =>
    settings.general.autoArchiveWorkspacesAfterDays,
  _ => throw ArgumentError.value(path, 'path', 'MCP setting is not exposed'),
};

/// Validated scalar settings. Dangerous confirmation switches and automatic
/// archiving are discoverable/read-only; never silently disable safeguards.
bool mcpWritableSettingValue(String path, Object? value) => switch (path) {
  'general.autoGitFetchEnabled' || 'editor.autosaveEnabled' => value is bool,
  'general.autoGitFetchIntervalMinutes' =>
    value is int && value >= 1 && value <= 60,
  'terminal.fontSize' =>
    value is num && value.isFinite && value >= 8 && value <= 48,
  'editor.tabSize' => value is int && value >= 1 && value <= 16,
  _ => false,
};

/// The same application controller that Settings UI already uses owns
/// persistence, portable configuration and UI notifications. CAS occurs
/// inside its serialized mutation lane to avoid overwriting another setting
/// change with a stale model. A caller must first read the expected value.
mixin _SettingsControllerMcpAccess on _$SettingsController {
  SettingsController get _controller => this as SettingsController;

  Object readMcpSetting(String path) => mcpSettingValue(state, path);

  /// MCP reads must observe the Host-owned latest persisted settings. A
  /// second Flutter process can have an old local controller snapshot even
  /// after the first process has completed an authorized CAS update.
  /// Serialize reloads with local mutations and fail closed on load errors;
  /// never return an old value while claiming it is current.
  Future<Object> readMcpSettingFresh(String path) async {
    // Enforce the reviewed projection before any repository I/O and await
    // pending local saves. A full repository load may silently fall back to
    // legacy Drift, which is not an authoritative cross-window MCP read.
    mcpSettingValue(state, path);
    await _controller._serialize(() async {});
    final repository = _controller._repository;
    if (repository is RuntimeSettingsRepository) {
      return repository.readAuthoritativeMcpSetting(path);
    }
    // Isolated offline controller tests have only a local repository.
    await _controller._serialize(() async {
      state = await repository.load();
    });
    return mcpSettingValue(state, path);
  }

  Future<Object> updateMcpSetting({
    required String path,
    required Object? expectedValue,
    required Object? value,
  }) async {
    if (!mcpWritableSettingValue(path, value)) {
      throw ArgumentError.value(path, 'path', 'MCP setting is not writable');
    }
    await _controller._serialize(() async {
      final current = mcpSettingValue(state, path);
      if (!_sameMcpSettingValue(current, expectedValue)) {
        throw StateError('mcp_setting_conflict');
      }
      if (_sameMcpSettingValue(current, value)) return;
      final next = switch (path) {
        'general.autoGitFetchEnabled' => state.copyWith(
          general: state.general.copyWith(autoGitFetchEnabled: value as bool),
        ),
        'general.autoGitFetchIntervalMinutes' => state.copyWith(
          general: state.general.copyWith(
            autoGitFetchIntervalMinutes: value as int,
          ),
        ),
        'terminal.fontSize' => state.copyWith(
          terminal: state.terminal.copyWith(
            fontSize: (value as num).toDouble(),
          ),
        ),
        'editor.autosaveEnabled' => state.copyWith(
          editor: state.editor.copyWith(autosaveEnabled: value as bool),
        ),
        'editor.tabSize' => state.copyWith(
          editor: state.editor.copyWith(tabSize: value as int),
        ),
        _ => throw ArgumentError.value(
          path,
          'path',
          'MCP setting is not writable',
        ),
      };
      await _controller._save(next);
    });
    return mcpSettingValue(state, path);
  }
}

bool _sameMcpSettingValue(Object current, Object? other) {
  if (current is num) {
    return other is num &&
        other.isFinite &&
        current.toDouble() == other.toDouble();
  }
  return current == other;
}
