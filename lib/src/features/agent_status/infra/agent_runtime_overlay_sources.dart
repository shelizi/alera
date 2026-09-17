part of 'agent_runtime_overlay_service.dart';

extension _AgentRuntimeOverlaySources on AgentRuntimeOverlayService {
  _OverlaySource _resolveAmpSource() {
    final sourceValue = _trimmedEnvironmentValue('ALERA_AMP_SOURCE_CONFIG_DIR');
    if (sourceValue != null) {
      return _OverlaySource(sourceValue, isExplicit: true);
    }

    final ampConfigValue = _trimmedEnvironmentValue('AMP_CONFIG_DIR');
    final overlayValue = _trimmedEnvironmentValue('ALERA_AMP_CONFIG_DIR');
    if (ampConfigValue != null &&
        (overlayValue == null || !_samePath(ampConfigValue, overlayValue))) {
      return _OverlaySource(ampConfigValue, isExplicit: true);
    }

    final xdgConfigHome = _trimmedEnvironmentValue('XDG_CONFIG_HOME');
    if (xdgConfigHome != null) {
      final candidate = p.join(xdgConfigHome, 'amp');
      if (overlayValue == null || !_samePath(candidate, overlayValue)) {
        return _OverlaySource(candidate, isExplicit: false);
      }
    }

    return _OverlaySource(_defaultAmpConfigDir(), isExplicit: false);
  }

  _OverlaySource _resolveSource({
    required String publicEnvKey,
    required String overlayEnvKey,
    required String sourceEnvKey,
    required String defaultSourcePath,
  }) {
    final sourceValue = _trimmedEnvironmentValue(sourceEnvKey);
    if (sourceValue != null) {
      return _OverlaySource(sourceValue, isExplicit: true);
    }

    final publicValue = _trimmedEnvironmentValue(publicEnvKey);
    final overlayValue = _trimmedEnvironmentValue(overlayEnvKey);
    if (publicValue != null &&
        (overlayValue == null || !_samePath(publicValue, overlayValue))) {
      return _OverlaySource(publicValue, isExplicit: true);
    }

    final startupValue = _readShellStartupEnvVar(publicEnvKey);
    if (startupValue != null) {
      return _OverlaySource(startupValue, isExplicit: true);
    }

    return _OverlaySource(defaultSourcePath, isExplicit: false);
  }

  String? _trimmedEnvironmentValue(String key) {
    final value = _environment[key]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

  String _defaultOpenCodeConfigDir() {
    if (_platform == ManagedAgentHookPlatform.windows) {
      final appData = _trimmedEnvironmentValue('APPDATA');
      if (appData != null) {
        return p.join(appData, 'opencode');
      }
    }
    return p.join(_homeDirectory, '.config', 'opencode');
  }

  String _defaultAmpConfigDir() {
    if (_platform == ManagedAgentHookPlatform.windows) {
      final userProfile = _trimmedEnvironmentValue('USERPROFILE');
      if (userProfile != null) {
        return p.join(userProfile, '.config', 'amp');
      }
    }
    return p.join(_homeDirectory, '.config', 'amp');
  }

  String _overlayRoot(Directory support, String agentKey) {
    return p.join(support.path, 'agent-runtime-overlays', agentKey);
  }

  Directory _overlayDirectory(String root, String terminalSessionId) {
    final hash = sha256
        .convert(utf8.encode(terminalSessionId))
        .toString()
        .substring(0, 32);
    return Directory(p.join(root, hash));
  }

  bool _sourceExists(String path) {
    return FileSystemEntity.isDirectorySync(path) ||
        FileSystemEntity.isFileSync(path) ||
        Link(path).existsSync();
  }

  bool _samePath(String left, String right) {
    return p.normalize(p.absolute(left)) == p.normalize(p.absolute(right));
  }
}
