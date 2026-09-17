import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:alera/src/rust/api/agent_runtime_overlay.dart' as native;
import 'package:crypto/crypto.dart';
import 'package:meta/meta.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

part 'agent_runtime_overlay_prepare.dart';
part 'agent_runtime_overlay_wrappers.dart';
part 'agent_runtime_overlay_sources.dart';
part 'agent_runtime_overlay_shell.dart';

typedef AgentOverlayApplicationSupportDirectoryResolver =
    Future<Directory> Function();

typedef AgentRuntimeOverlayNativePreparer =
    Future<native.AgentRuntimeOverlayResult> Function({
      required native.AgentRuntimeOverlayRequest request,
    });
typedef AgentRuntimeOverlayNativeCleaner =
    Future<native.AgentRuntimeOverlayCleanupResult> Function({
      required List<native.AgentRuntimeOverlayCleanupTarget> targets,
    });

final class const AgentRuntimeOverlayPreparation({
  required final Map<String, String> environment,
  final String? overlayPath,
  final String? sourcePath,
});

final class AgentRuntimeOverlayService({
  String? homeDirectory,
  ManagedAgentHookPlatform? platform,
  Map<String, String>? environment,
  AgentOverlayApplicationSupportDirectoryResolver? applicationSupportDirectory,
  @visibleForTesting AgentRuntimeOverlayNativePreparer? nativePreparer,
  @visibleForTesting AgentRuntimeOverlayNativeCleaner? nativeCleaner,
}) {
  this
    : _environment = environment ?? Platform.environment,
      _homeDirectory = homeDirectory ?? _resolveHome(environment),
      _platform =
          platform ??
          (Platform.isWindows
              ? ManagedAgentHookPlatform.windows
              : ManagedAgentHookPlatform.posix),
      _applicationSupportDirectory =
          applicationSupportDirectory ?? getApplicationSupportDirectory,
      _nativePreparer = nativePreparer ?? native.prepareAgentRuntimeOverlay,
      _nativeCleaner = nativeCleaner ?? native.clearAgentRuntimeOverlays;

  final Map<String, String> _environment;
  final String _homeDirectory;
  final ManagedAgentHookPlatform _platform;
  final AgentOverlayApplicationSupportDirectoryResolver
  _applicationSupportDirectory;
  final AgentRuntimeOverlayNativePreparer _nativePreparer;
  final AgentRuntimeOverlayNativeCleaner _nativeCleaner;

  Future<AgentRuntimeOverlayPreparation> prepareOpenCodeForTerminalLaunch({
    required String terminalSessionId,
    bool includeV1Plugin = true,
    bool includeV2Plugin = false,
  }) {
    // v1 and v2 share OPENCODE_CONFIG_DIR. Write only the plugins the user
    // enabled so auto-discovery in either binary does not load the other API.
    final managedFiles = <String, String>{
      if (includeV1Plugin)
        'alera-agent-status.js': aleraOpenCodeStatusPluginSource(),
      if (includeV2Plugin)
        'alera-agent-status-v2.js': aleraOpenCode2StatusPluginSource(),
    };
    if (managedFiles.isEmpty) {
      return Future<AgentRuntimeOverlayPreparation>.value(
        const AgentRuntimeOverlayPreparation(environment: <String, String>{}),
      );
    }
    return _prepareOverlay(
      agentKey: 'opencode',
      terminalSessionId: terminalSessionId,
      publicEnvKey: 'OPENCODE_CONFIG_DIR',
      overlayEnvKey: 'ALERA_OPENCODE_CONFIG_DIR',
      sourceEnvKey: 'ALERA_OPENCODE_SOURCE_CONFIG_DIR',
      defaultSourcePath: _defaultOpenCodeConfigDir(),
      managedSubdirectory: 'plugins',
      managedFiles: managedFiles,
    );
  }

  Future<AgentRuntimeOverlayPreparation> preparePiForTerminalLaunch({
    required String terminalSessionId,
  }) async {
    final source = _resolveSource(
      publicEnvKey: 'PI_CODING_AGENT_DIR',
      overlayEnvKey: 'ALERA_PI_CODING_AGENT_DIR',
      sourceEnvKey: 'ALERA_PI_SOURCE_AGENT_DIR',
      defaultSourcePath: p.join(_homeDirectory, '.pi', 'agent'),
    );

    // Pi stores mutable session state under PI_CODING_AGENT_DIR/sessions.
    // Keep the real agent directory so resume sees the same sessions as an
    // external Pi launch, and install only Alera's managed extension in place.
    try {
      await _nativePreparer(
        request: native.AgentRuntimeOverlayRequest(
          overlayRoot: null,
          overlayPath: null,
          mirrorPath: null,
          sourcePath: source.path,
          managedSubdirectory: null,
          managedFileNames: const <String>[],
          managedFiles: <native.AgentRuntimeOverlayManagedFile>[
            native.AgentRuntimeOverlayManagedFile(
              path: p.join(source.path, 'extensions', 'alera-agent-status.ts'),
              allowedRoot: source.path,
              content: aleraPiStatusExtensionSource(),
              writeMode: native.AgentRuntimeOverlayWriteMode.replace,
              executable: false,
            ),
          ],
        ),
      );
    } catch (_) {
      return AgentRuntimeOverlayPreparation(
        sourcePath: source.isExplicit ? source.path : null,
        environment: <String, String>{
          if (source.isExplicit) 'PI_CODING_AGENT_DIR': source.path,
        },
      );
    }

    return AgentRuntimeOverlayPreparation(
      sourcePath: source.path,
      environment: <String, String>{
        if (source.isExplicit) 'PI_CODING_AGENT_DIR': source.path,
      },
    );
  }

  Future<AgentRuntimeOverlayPreparation> prepareCopilotForTerminalLaunch({
    required String terminalSessionId,
  }) async {
    final source = _resolveSource(
      publicEnvKey: 'COPILOT_HOME',
      overlayEnvKey: 'ALERA_COPILOT_HOME',
      sourceEnvKey: 'ALERA_COPILOT_SOURCE_HOME',
      defaultSourcePath: p.join(_homeDirectory, '.copilot'),
    );

    // Copilot keeps mutable resume state in COPILOT_HOME (session-state,
    // open-sessions-state.json and session-store.db). A per-session overlay can
    // therefore fork or hide the user's existing sessions when a link falls
    // back to a copy. Install the managed hook directly into the effective user
    // home instead and leave the default COPILOT_HOME untouched.
    try {
      final status = ManagedAgentHookInstallService(
        homeDirectory: source.path,
        platform: _platform,
        environment: <String, String>{
          ..._environment,
          'HOME': source.path,
          'COPILOT_HOME': source.path,
        },
      ).install(.copilot);
      if (status.state == ManagedAgentHookInstallState.error) {
        throw StateError(status.detail ?? 'Could not install Copilot hooks.');
      }
    } catch (_) {
      return AgentRuntimeOverlayPreparation(
        sourcePath: source.isExplicit ? source.path : null,
        environment: <String, String>{
          if (source.isExplicit) 'COPILOT_HOME': source.path,
        },
      );
    }

    return AgentRuntimeOverlayPreparation(
      sourcePath: source.path,
      environment: <String, String>{
        if (source.isExplicit) 'COPILOT_HOME': source.path,
      },
    );
  }

  Future<AgentRuntimeOverlayPreparation> prepareAmpForTerminalLaunch({
    required String terminalSessionId,
  }) async {
    final source = _resolveAmpSource();
    final support = await _applicationSupportDirectory();
    final root = _overlayRoot(support, 'amp');
    final overlay = _overlayDirectory(root, terminalSessionId);
    final xdgConfigHome = p.join(overlay.path, 'xdg');
    final ampConfigDir = p.join(xdgConfigHome, 'amp');
    final settingsPath = p.join(ampConfigDir, 'settings.json');
    final wrapperBin = _wrapperBinDirectory(support, terminalSessionId);
    final wrapperRoot = p.dirname(wrapperBin.path);
    final wrapperPath = p.join(wrapperBin.path, _wrapperFileName('amp'));
    try {
      final result = await _nativePreparer(
        request: native.AgentRuntimeOverlayRequest(
          overlayRoot: root,
          overlayPath: overlay.path,
          mirrorPath: ampConfigDir,
          sourcePath: source.path,
          managedSubdirectory: 'plugins',
          managedFileNames: const <String>['alera-agent-status.ts'],
          managedFiles: <native.AgentRuntimeOverlayManagedFile>[
            native.AgentRuntimeOverlayManagedFile(
              path: p.join(ampConfigDir, 'plugins', 'alera-agent-status.ts'),
              allowedRoot: ampConfigDir,
              content: aleraAmpStatusPluginSource(),
              writeMode: native.AgentRuntimeOverlayWriteMode.replace,
              executable: false,
            ),
            native.AgentRuntimeOverlayManagedFile(
              path: settingsPath,
              allowedRoot: ampConfigDir,
              content: '{}\n',
              writeMode: native.AgentRuntimeOverlayWriteMode.createIfMissing,
              executable: false,
            ),
            native.AgentRuntimeOverlayManagedFile(
              path: wrapperPath,
              allowedRoot: wrapperRoot,
              content: _ampWrapperSource(
                xdgConfigHome: xdgConfigHome,
                settingsFile: settingsPath,
                wrapperDirectory: wrapperBin.path,
              ),
              writeMode: native.AgentRuntimeOverlayWriteMode.replace,
              executable: _platform != ManagedAgentHookPlatform.windows,
            ),
          ],
        ),
      );
      return AgentRuntimeOverlayPreparation(
        overlayPath: overlay.path,
        sourcePath: result.sourceExists ? source.path : null,
        environment: <String, String>{
          'ALERA_AMP_CONFIG_DIR': ampConfigDir,
          if (result.sourceExists) 'ALERA_AMP_SOURCE_CONFIG_DIR': source.path,
          'ALERA_AGENT_WRAPPER_PATH': wrapperBin.path,
        },
      );
    } catch (_) {
      return const AgentRuntimeOverlayPreparation(
        environment: <String, String>{},
      );
    }
  }

  Future<void> clearTerminalOverlays(String terminalSessionId) async {
    final support = await _applicationSupportDirectory();
    final targets = <native.AgentRuntimeOverlayCleanupTarget>[];
    for (final agentKey in const <String>[
      'opencode',
      'pi',
      'copilot',
      'cursor',
      'amp',
      'wrappers',
    ]) {
      final root = _overlayRoot(support, agentKey);
      targets.add(
        native.AgentRuntimeOverlayCleanupTarget(
          overlayRoot: root,
          overlayPath: _overlayDirectory(root, terminalSessionId).path,
        ),
      );
    }
    await _nativeCleaner(targets: targets);
  }
}

final class const _OverlaySource(
  final String path, {
  required final bool isExplicit,
});

final class const _ShellValue(final String text, final String? quoted);

String _resolveHome(Map<String, String>? environment) {
  final env = environment ?? Platform.environment;
  // coverage:ignore-start
  // Host-OS branch; injected platform tests cover the Windows overlay paths,
  // while home resolution itself follows the current process platform.
  if (Platform.isWindows) {
    final profile = env['USERPROFILE']?.trim();
    if (profile != null && profile.isNotEmpty) {
      return profile;
    }
  }
  // coverage:ignore-end
  final home = env['HOME']?.trim();
  if (home != null && home.isNotEmpty) {
    return home;
  }
  final profile = env['USERPROFILE']?.trim();
  if (profile != null && profile.isNotEmpty) {
    return profile;
  }
  return Directory.current.path;
}
