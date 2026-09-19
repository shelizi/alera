part of 'managed_agent_hook_installer.dart';

final Map<String, _ManagedAgentHookAdapter> _managedAgentHookAdapters =
    <String, _ManagedAgentHookAdapter>{
      'codex': _codexManagedAgentHookAdapter,
      'claude': _claudeManagedAgentHookAdapter,
      'copilot': _copilotManagedAgentHookAdapter,
      'cursor': _cursorManagedAgentHookAdapter,
      'agy': _agyManagedAgentHookAdapter,
      'opencode': _openCodeManagedAgentHookAdapter,
      'opencode2': _openCode2ManagedAgentHookAdapter,
      'pi': _piManagedAgentHookAdapter,
      'amp': _ampManagedAgentHookAdapter,
      'grok': _grokManagedAgentHookAdapter,
      'devin': _devinManagedAgentHookAdapter,
      'fx': _fxManagedAgentHookAdapter,
    };

_ManagedAgentHookAdapter _managedAgentHookAdapterFor(AgentType agentType) {
  final adapter = _managedAgentHookAdapters[agentType.key];
  if (adapter == null) {
    throw StateError('Missing managed-hook adapter for ${agentType.key}.');
  }
  return adapter;
}

extension _ManagedAgentHookDescriptors on ManagedAgentHookInstallService {
  ManagedAgentHookInstallStatus _runtimeOnlyStatus(AgentType agentType) {
    final builder = _managedAgentHookAdapterFor(agentType).runtimeOnlyStatus;
    return builder == null
        ? _unsupportedStrategyStatus(agentType)
        : builder(this);
  }

  ManagedAgentHookInstallStatus _unsupportedStrategyStatus(
    AgentType agentType,
  ) {
    return ManagedAgentHookInstallStatus(
      agentType: agentType,
      state: .error,
      configPath: p.join(_homeDirectory, '.alera', 'agent-hooks'),
      managedHooksPresent: false,
      detail: '${agentDisplayName(agentType)} has no managed hook strategy.',
    );
  }

  _AgentHookDescriptor _descriptor(AgentType agentType) {
    final adapter = _managedAgentHookAdapterFor(agentType);
    final extension = _platform == ManagedAgentHookPlatform.windows
        ? adapter.windowsScriptExtension
        : 'sh';
    final scriptFileName = 'alera-${agentType.key}-hook.$extension';
    final scriptPath = p.join(
      _homeDirectory,
      '.alera',
      'agent-hooks',
      scriptFileName,
    );
    final builder = adapter.jsonDescriptor;
    if (builder == null) {
      throw ArgumentError.value(
        agentType,
        'agentType',
        'This agent does not use a JSON hook descriptor.',
      );
    }
    return builder(this, scriptFileName, scriptPath);
  }

  _ManagedHookArtifact? _managedArtifact(AgentType agentType) {
    return _managedAgentHookAdapterFor(agentType).managedArtifact?.call(this);
  }

  ManagedAgentHookInstallStatus _managedArtifactStatus(
    _ManagedHookArtifact artifact,
  ) {
    final file = File(artifact.path);
    if (!file.existsSync()) {
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .notInstalled,
        configPath: artifact.path,
        managedHooksPresent: false,
      );
    }
    late final String content;
    try {
      content = file.readAsStringSync();
    } catch (_) {
      // coverage:ignore-start
      // File permission races are platform/filesystem dependent; status tests
      // cover missing, managed, stale, and unmanaged artifact contents.
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .error,
        configPath: artifact.path,
        managedHooksPresent: false,
        detail: 'Could not read ${artifact.label}.',
      );
      // coverage:ignore-end
    }
    if (!content.contains(_managedArtifactMarker)) {
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .error,
        configPath: artifact.path,
        managedHooksPresent: false,
        detail:
            'Existing ${artifact.label} is not Alera-managed. Rename it before enabling this hook.',
      );
    }
    if (content == artifact.content) {
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .installed,
        configPath: artifact.path,
        managedHooksPresent: true,
      );
    }
    return ManagedAgentHookInstallStatus(
      agentType: artifact.agentType,
      state: .partial,
      configPath: artifact.path,
      managedHooksPresent: true,
      detail: 'Managed ${artifact.label} needs to be updated.',
    );
  }

  ManagedAgentHookInstallStatus _removeManagedArtifact(
    _ManagedHookArtifact artifact,
  ) {
    final file = File(artifact.path);
    if (!file.existsSync()) {
      return _managedArtifactStatus(artifact);
    }
    late final String content;
    try {
      content = file.readAsStringSync();
    } catch (_) {
      // coverage:ignore-start
      // File permission races are platform/filesystem dependent; remove tests
      // cover the managed and unmanaged artifact paths.
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .error,
        configPath: artifact.path,
        managedHooksPresent: false,
        detail: 'Could not read ${artifact.label}.',
      );
      // coverage:ignore-end
    }
    if (!content.contains(_managedArtifactMarker)) {
      return ManagedAgentHookInstallStatus(
        agentType: artifact.agentType,
        state: .error,
        configPath: artifact.path,
        managedHooksPresent: false,
        detail:
            'Existing ${artifact.label} is not Alera-managed. Refusing to remove it.',
      );
    }
    file.deleteSync();
    return _managedArtifactStatus(artifact);
  }

  void _writeManagedArtifact(_ManagedHookArtifact artifact) {
    final file = File(artifact.path);
    file.parent.createSync(recursive: true);
    if (file.existsSync() && file.readAsStringSync() == artifact.content) {
      return;
    }
    final tmpPath = p.join(
      file.parent.path,
      '.${DateTime.now().microsecondsSinceEpoch}.tmp',
    );
    final tmp = File(tmpPath)..writeAsStringSync(artifact.content);
    tmp.renameSync(artifact.path);
  }
}
