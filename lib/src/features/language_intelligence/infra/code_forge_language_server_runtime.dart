import 'dart:async';
import 'dart:io';

import 'package:code_forge/code_forge.dart';

import '../../../shared/infra/process/command_path_probe.dart';
import '../application/language_server_runtime.dart';
import '../application/managed_language_server_installer.dart';
import '../domain/language_capability.dart';
import '../domain/language_intelligence_settings.dart';
import '../domain/language_provider_descriptor.dart';
import 'managed_language_server_installer.dart';

typedef CodeForgeEnvironmentReader = Map<String, String> Function();
typedef CodeForgeExecutableExists = bool Function(String path);
typedef CodeForgeLanguageServerTransportFactory =
    Future<CodeForgeLanguageServerTransport> Function(
      CodeForgeLanguageServerLaunchSpec spec,
    );

final class CodeForgeLanguageServerLaunchSpec {
  CodeForgeLanguageServerLaunchSpec({
    required this.executable,
    required this.workspaceRoot,
    required this.bootstrapLanguageId,
    required Iterable<String> arguments,
    required Map<String, String> environment,
    required Iterable<LanguageCapability> capabilities,
  }) : arguments = List<String>.unmodifiable(arguments),
       environment = Map<String, String>.unmodifiable(environment),
       capabilities = Set<LanguageCapability>.unmodifiable(capabilities);

  final String executable;
  final String workspaceRoot;
  final String bootstrapLanguageId;
  final List<String> arguments;
  final Map<String, String> environment;
  final Set<LanguageCapability> capabilities;
}

abstract interface class CodeForgeLanguageServerTransport {
  Future<void> initialize();

  Future<void> shutdown();

  Future<void> exitServer();

  Future<int> get processExitCode;

  Stream<Map<String, dynamic>> get responses;

  Future<Map<String, dynamic>> sendRequest({
    required String method,
    required Map<String, dynamic> params,
  });

  Future<void> sendNotification({
    required String method,
    required Map<String, dynamic> params,
  });

  Future<void> sendResponse({required Object id, Object? result});

  Future<void> sendErrorResponse({
    required Object id,
    required int code,
    required String message,
    Object? data,
  });

  void dispose();
}

final class CodeForgeLanguageServerSession
    implements LanguageServerRuntimeSession {
  const CodeForgeLanguageServerSession._(
    this.transport,
    this.serverRequestSubscription,
  );

  final CodeForgeLanguageServerTransport transport;
  final StreamSubscription<Map<String, dynamic>> serverRequestSubscription;
}

final class CodeForgeLanguageServerRuntime
    implements LanguageServerRuntimePort {
  factory CodeForgeLanguageServerRuntime({
    CodeForgeEnvironmentReader? environmentReader,
    bool? isWindows,
    CodeForgeExecutableExists? executableExists,
    CodeForgeLanguageServerTransportFactory? transportFactory,
    ManagedLanguageServerInstallerPort? managedInstaller,
  }) => CodeForgeLanguageServerRuntime._(
    environmentReader: environmentReader ?? _platformEnvironment,
    isWindows: isWindows ?? Platform.isWindows,
    executableExists: executableExists,
    transportFactory: transportFactory ?? _startCodeForgeTransport,
    managedInstaller:
        managedInstaller ??
        ManagedLanguageServerInstaller(
          environmentReader: environmentReader ?? _platformEnvironment,
          isWindows: isWindows ?? Platform.isWindows,
          executableExists: executableExists,
        ),
  );

  const CodeForgeLanguageServerRuntime._({
    required this._environmentReader,
    required this._isWindows,
    required this._executableExists,
    required this._transportFactory,
    required this._managedInstaller,
  });

  final CodeForgeEnvironmentReader _environmentReader;
  final bool _isWindows;
  final CodeForgeExecutableExists? _executableExists;
  final CodeForgeLanguageServerTransportFactory _transportFactory;
  final ManagedLanguageServerInstallerPort _managedInstaller;

  @override
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  }) async {
    if (target == LanguageServerTarget.remoteWorkspace) {
      return const LanguageServerExecutableMissing(
        reason:
            'Remote workspace language servers are not supported by the local '
            'CodeForge runtime.',
      );
    }
    if (provider.executableResolutionPolicy !=
        LanguageExecutableResolutionPolicy.explicitOverrideThenPath) {
      return LanguageServerExecutableMissing(
        reason:
            'Provider ${provider.id} does not declare a supported executable '
            'resolution policy.',
      );
    }

    final environment = _environmentReader();
    final override = settings.executablePath?.trim();
    if (override != null && override.isNotEmpty) {
      if (_looksLikePath(override)) {
        if (_pathExists(override)) {
          return LanguageServerExecutableResolved(override);
        }
      } else {
        final resolved = _resolveCommand(override, environment);
        if (resolved != null) {
          return LanguageServerExecutableResolved(resolved);
        }
      }
      return LanguageServerExecutableMissing(
        reason: 'Configured executable was not found: $override',
      );
    }

    for (final candidate in provider.executableCandidates) {
      final resolved = _resolveCommand(candidate, environment);
      if (resolved != null) {
        return LanguageServerExecutableResolved(resolved);
      }
    }
    final managed = await _managedInstaller.ensureInstalled(provider);
    switch (managed) {
      case ManagedLanguageServerInstalled(:final executable):
        if (_pathExists(executable)) {
          return LanguageServerExecutableResolved(executable);
        }
        throw StateError(
          'Managed language server ${provider.id} reported an executable that '
          'does not exist: $executable',
        );
      case ManagedLanguageServerUnavailable(:final reason):
        final tried = provider.executableCandidates.isEmpty
            ? 'no executable candidates were configured'
            : 'tried ${provider.executableCandidates.join(', ')}';
        return LanguageServerExecutableMissing(
          reason: 'No executable found for ${provider.id}; $tried. $reason',
        );
    }
  }

  @override
  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  ) async {
    if (request.target != LanguageServerTarget.localWorkspace) {
      throw UnsupportedError(
        'CodeForge stdio language servers can only run for local workspaces.',
      );
    }
    if (request.provider.kind != LanguageProviderKind.semanticServer) {
      throw ArgumentError.value(
        request.provider.id,
        'provider',
        'CodeForge runtime only starts semantic-server providers.',
      );
    }
    final languageIds =
        request.provider.languages
            .map((language) => language.value)
            .toList(growable: false)
          ..sort();
    if (languageIds.isEmpty) {
      throw StateError('Provider ${request.provider.id} has no languages.');
    }

    final transport = await _transportFactory(
      CodeForgeLanguageServerLaunchSpec(
        executable: request.executable,
        workspaceRoot: request.workspaceRoot,
        bootstrapLanguageId: languageIds.first,
        arguments: request.arguments,
        environment: request.environment,
        capabilities: request.provider.capabilities,
      ),
    );
    final serverRequestSubscription = _listenForServerRequests(
      transport,
      workspaceRoot: request.workspaceRoot,
      isWindows: _isWindows,
    );
    try {
      await transport.initialize();
    } catch (_) {
      await serverRequestSubscription.cancel();
      _disposeQuietly(transport);
      rethrow;
    }
    return CodeForgeLanguageServerSession._(
      transport,
      serverRequestSubscription,
    );
  }

  @override
  Stream<LanguageServerExit> observeExit(LanguageServerRuntimeSession session) {
    final codeForgeSession = _requireSession(session);
    return _observeTransportExit(codeForgeSession.transport);
  }

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {
    final codeForgeSession = _requireSession(session);
    final transport = codeForgeSession.transport;
    try {
      await transport.shutdown();
    } catch (_) {
      // A crashed or already-stopped server cannot answer shutdown.
    }
    try {
      await transport.exitServer();
    } catch (_) {
      // Best-effort graceful exit; dispose below is the final process cleanup.
    }
    await codeForgeSession.serverRequestSubscription.cancel();
    _disposeQuietly(transport);
  }

  bool _pathExists(String value) => executablePathExists(
    value,
    isWindows: _isWindows,
    executableExists: _executableExists,
  );

  String? _resolveCommand(String value, Map<String, String> environment) =>
      resolveCommandOnPath(
        value,
        environment: environment,
        isWindows: _isWindows,
        executableExists: _executableExists,
      );

  static bool _looksLikePath(String value) =>
      value.contains('/') || value.contains(r'\');

  static CodeForgeLanguageServerSession _requireSession(
    LanguageServerRuntimeSession session,
  ) {
    if (session is! CodeForgeLanguageServerSession) {
      throw ArgumentError.value(
        session,
        'session',
        'Session was not created by CodeForgeLanguageServerRuntime.',
      );
    }
    return session;
  }

  static Stream<LanguageServerExit> _observeTransportExit(
    CodeForgeLanguageServerTransport transport,
  ) async* {
    try {
      final exitCode = await transport.processExitCode;
      yield LanguageServerExit(exitCode: exitCode);
    } catch (error) {
      yield LanguageServerExit(error: error);
    }
  }

  static void _disposeQuietly(CodeForgeLanguageServerTransport transport) {
    try {
      transport.dispose();
    } catch (_) {
      // Process cleanup is best effort after shutdown/init failure.
    }
  }

  static StreamSubscription<Map<String, dynamic>> _listenForServerRequests(
    CodeForgeLanguageServerTransport transport, {
    required String workspaceRoot,
    required bool isWindows,
  }) => transport.responses.listen((message) {
    final id = message['id'];
    final method = message['method'];
    if (id == null || method is! String) {
      return;
    }
    unawaited(
      _respondToServerRequest(
        transport,
        id: id,
        method: method,
        params: message['params'],
        workspaceRoot: workspaceRoot,
        isWindows: isWindows,
      ),
    );
  });

  static Future<void> _respondToServerRequest(
    CodeForgeLanguageServerTransport transport, {
    required Object id,
    required String method,
    required Object? params,
    required String workspaceRoot,
    required bool isWindows,
  }) async {
    try {
      switch (method) {
        case 'workspace/configuration':
          final items = params is Map ? params['items'] : null;
          final count = items is List ? items.length : 0;
          await transport.sendResponse(
            id: id,
            result: List<Map<String, dynamic>>.generate(
              count,
              (_) => <String, dynamic>{},
              growable: false,
            ),
          );
          return;
        case 'client/registerCapability':
        case 'client/unregisterCapability':
        case 'window/workDoneProgress/create':
        case 'window/showMessageRequest':
        case 'workspace/codeLens/refresh':
        case 'workspace/semanticTokens/refresh':
        case 'workspace/inlayHint/refresh':
        case 'workspace/diagnostic/refresh':
          await transport.sendResponse(id: id, result: null);
          return;
        case 'workspace/workspaceFolders':
          await transport.sendResponse(
            id: id,
            result: <Map<String, dynamic>>[
              <String, dynamic>{
                'uri': Uri.directory(
                  workspaceRoot,
                  windows: isWindows,
                ).toString(),
                'name': 'workspace',
              },
            ],
          );
          return;
        case 'workspace/applyEdit':
          await transport.sendResponse(
            id: id,
            result: <String, dynamic>{
              'applied': false,
              'failureReason': 'Workspace edits are not supported by Alera language intelligence.',
            },
          );
          return;
        case 'window/showDocument':
          await transport.sendResponse(
            id: id,
            result: const <String, dynamic>{'success': false},
          );
          return;
        default:
          await transport.sendErrorResponse(
            id: id,
            code: -32601,
            message: 'Unsupported language-server request: $method',
          );
      }
    } catch (_) {
      // The language-server process may exit while a response is being sent.
    }
  }
}

Map<String, String> _platformEnvironment() => Platform.environment;

Future<CodeForgeLanguageServerTransport> _startCodeForgeTransport(
  CodeForgeLanguageServerLaunchSpec spec,
) async {
  final config = await LspStdioConfig.start(
    executable: spec.executable,
    workspacePath: spec.workspaceRoot,
    languageId: spec.bootstrapLanguageId,
    args: spec.arguments,
    environment: spec.environment,
    capabilities: _codeForgeCapabilities(spec.capabilities),
  );
  return _LspStdioCodeForgeTransport(config);
}

LspClientCapabilities _codeForgeCapabilities(
  Set<LanguageCapability> capabilities,
) {
  final definitionFamily =
      capabilities.contains(LanguageCapability.definition) ||
      capabilities.contains(LanguageCapability.declaration) ||
      capabilities.contains(LanguageCapability.typeDefinition) ||
      capabilities.contains(LanguageCapability.implementation);
  return LspClientCapabilities(
    semanticHighlighting: capabilities.contains(
      LanguageCapability.semanticTokens,
    ),
    codeCompletion: capabilities.contains(LanguageCapability.completion),
    hoverInfo: capabilities.contains(LanguageCapability.hover),
    codeAction: false,
    signatureHelp: false,
    documentColor: false,
    documentHighlight: false,
    codeFolding: capabilities.contains(LanguageCapability.folding),
    inlayHint: false,
    goToDefinition: definitionFamily,
    rename: capabilities.contains(LanguageCapability.rename),
  );
}

final class _LspStdioCodeForgeTransport
    implements CodeForgeLanguageServerTransport {
  const _LspStdioCodeForgeTransport(this._config);

  final LspStdioConfig _config;

  @override
  Future<void> initialize() => _config.initialize();

  @override
  Future<void> shutdown() => _config.shutdown();

  @override
  Future<void> exitServer() => _config.exitServer();

  @override
  Future<int> get processExitCode => _config.exitCode;

  @override
  Stream<Map<String, dynamic>> get responses => _config.responses;

  @override
  Future<Map<String, dynamic>> sendRequest({
    required String method,
    required Map<String, dynamic> params,
  }) => _config.sendRequest(method: method, params: params);

  @override
  Future<void> sendNotification({
    required String method,
    required Map<String, dynamic> params,
  }) => _config.sendNotification(method: method, params: params);

  @override
  Future<void> sendResponse({required Object id, Object? result}) async {
    await _config.sendResponse(id, result);
  }

  @override
  Future<void> sendErrorResponse({
    required Object id,
    required int code,
    required String message,
    Object? data,
  }) async {
    await _config.sendErrorResponse(
      id,
      code: code,
      message: message,
      data: data,
    );
  }

  @override
  void dispose() => _config.dispose();
}
