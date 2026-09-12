part of 'terminal_runtime.dart';

GhosttyTerminalShellLaunch _launchWithSanitizedAgentHookEnvironment(
  GhosttyTerminalShellLaunch launch,
  Map<String, String>? agentHookEnvironment,
) {
  final environment = <String, String>{...?launch.environment};
  _restoreOrStripManagedOverlayEnvironment(
    environment,
    primary: 'OPENCODE_CONFIG_DIR',
    overlay: 'ALERA_OPENCODE_CONFIG_DIR',
    source: 'ALERA_OPENCODE_SOURCE_CONFIG_DIR',
  );
  _restoreOrStripManagedOverlayEnvironment(
    environment,
    primary: 'PI_CODING_AGENT_DIR',
    overlay: 'ALERA_PI_CODING_AGENT_DIR',
    source: 'ALERA_PI_SOURCE_AGENT_DIR',
  );
  _restoreOrStripManagedOverlayEnvironment(
    environment,
    primary: 'COPILOT_HOME',
    overlay: 'ALERA_COPILOT_HOME',
    source: 'ALERA_COPILOT_SOURCE_HOME',
  );
  _stripManagedWrapperPath(environment);
  environment.removeWhere(_isAleraAgentHookEnvironmentKey);
  if (agentHookEnvironment != null) {
    environment.addAll(agentHookEnvironment);
  }
  _applyManagedWrapperPath(environment);
  return GhosttyTerminalShellLaunch(
    label: launch.label,
    shell: launch.shell,
    arguments: launch.arguments,
    environment: environment.isEmpty ? null : environment,
    setupCommand: launch.setupCommand,
  );
}

void _restoreOrStripManagedOverlayEnvironment(
  Map<String, String> environment, {
  required String primary,
  required String overlay,
  required String source,
}) {
  final sourceValue = environment[source];
  final overlayValue = environment[overlay];
  if (sourceValue != null && sourceValue.isNotEmpty) {
    environment[primary] = sourceValue;
  } else if (overlayValue != null && environment[primary] == overlayValue) {
    environment.remove(primary);
  }
  environment.remove(overlay);
  environment.remove(source);
}

void _stripManagedWrapperPath(Map<String, String> environment) {
  final wrapperPath = environment['ALERA_AGENT_WRAPPER_PATH'];
  if (wrapperPath == null || wrapperPath.isEmpty) {
    return;
  }
  _removePathEntries(environment, _splitPathList(wrapperPath));
  environment.remove('ALERA_AGENT_WRAPPER_PATH');
}

void _applyManagedWrapperPath(Map<String, String> environment) {
  final wrapperPath = environment['ALERA_AGENT_WRAPPER_PATH'];
  if (wrapperPath == null || wrapperPath.isEmpty) {
    return;
  }
  final wrappers = _splitPathList(wrapperPath);
  if (wrappers.isEmpty) {
    return;
  }
  _removePathEntries(environment, wrappers);
  final current = _splitPathList(environment['PATH'] ?? '');
  environment['PATH'] = <String>[
    ...wrappers,
    ...current,
  ].join(_pathListSeparator);
}

void _removePathEntries(
  Map<String, String> environment,
  Iterable<String> entriesToRemove,
) {
  final remove = entriesToRemove.where((entry) => entry.isNotEmpty).toSet();
  if (remove.isEmpty) {
    return;
  }
  final path = environment['PATH'];
  if (path == null || path.isEmpty) {
    return;
  }
  final entries = _splitPathList(path)
      .where((entry) => !remove.contains(entry))
      .toList(growable: false);
  if (entries.isEmpty) {
    environment.remove('PATH');
  } else {
    environment['PATH'] = entries.join(_pathListSeparator);
  }
}

List<String> _splitPathList(String value) {
  if (value.isEmpty) {
    return const <String>[];
  }
  return value
      .split(_pathListSeparator)
      .where((entry) => entry.isNotEmpty)
      .toList(growable: false);
}

String get _pathListSeparator => Platform.isWindows ? ';' : ':';

@visibleForTesting
GhosttyTerminalShellLaunch launchWithSanitizedAgentHookEnvironmentForTesting(
  GhosttyTerminalShellLaunch launch,
  Map<String, String>? agentHookEnvironment,
) {
  return _launchWithSanitizedAgentHookEnvironment(launch, agentHookEnvironment);
}

bool _isAleraAgentHookEnvironmentKey(String key, String _) {
  return key.startsWith('ALERA_AGENT_HOOK_') ||
      key == 'ALERA_TERMINAL_SESSION_ID' ||
      key == 'ALERA_WORKSPACE_ID' ||
      key == 'ALERA_TAB_ID' ||
      key == 'ALERA_CODEX_HOME' ||
      key == 'ALERA_CLAUDE_CONFIG_DIR' ||
      key == 'ALERA_COPILOT_HOME' ||
      key == 'ALERA_COPILOT_SOURCE_HOME' ||
      key == 'ALERA_OPENCODE_CONFIG_DIR' ||
      key == 'ALERA_OPENCODE_SOURCE_CONFIG_DIR' ||
      key == 'ALERA_PI_CODING_AGENT_DIR' ||
      key == 'ALERA_PI_SOURCE_AGENT_DIR' ||
      key == 'ALERA_CURSOR_PLUGIN_DIR' ||
      key == 'ALERA_AMP_CONFIG_DIR' ||
      key == 'ALERA_AMP_SOURCE_CONFIG_DIR' ||
      key == 'ALERA_AGENT_WRAPPER_PATH';
}
