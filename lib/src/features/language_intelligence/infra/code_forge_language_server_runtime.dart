import 'dart:async';
import 'dart:io';

import 'package:code_forge/code_forge.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../../shared/infra/files/path_identity.dart';
import '../../../shared/infra/process/command_path_probe.dart';
import '../application/language_intelligence_activity.dart';
import '../application/language_server_runtime.dart';
import '../application/managed_language_server_installer.dart';
import '../application/workspace_nested_checkouts.dart';
import '../domain/language_capability.dart';
import '../domain/language_intelligence_settings.dart';
import '../domain/language_provider_descriptor.dart';
import 'managed_language_server_installer.dart';
import 'nested_checkout_exclusion_settings.dart';

typedef CodeForgeEnvironmentReader = Map<String, String> Function();
typedef CodeForgeExecutableExists = bool Function(String path);
typedef CodeForgeSupportDirectory = Future<Directory> Function();
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
    Map<String, dynamic> initializationOptions = const <String, dynamic>{},
    required Iterable<LanguageCapability> capabilities,
  }) : arguments = List<String>.unmodifiable(arguments),
       environment = Map<String, String>.unmodifiable(environment),
       initializationOptions = Map<String, dynamic>.unmodifiable(
         initializationOptions,
       ),
       capabilities = Set<LanguageCapability>.unmodifiable(capabilities);

  final String executable;
  final String workspaceRoot;
  final String bootstrapLanguageId;
  final List<String> arguments;
  final Map<String, String> environment;
  final Map<String, dynamic> initializationOptions;
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
    this.progressController,
  );

  final CodeForgeLanguageServerTransport transport;
  final StreamSubscription<Map<String, dynamic>> serverRequestSubscription;
  final StreamController<LanguageServerWorkProgress> progressController;
}

final class CodeForgeLanguageServerRuntime
    implements
        LanguageServerRuntimePort,
        LanguageServerProgressRuntimePort,
        LanguageServerWorkspaceStorageRuntimePort {
  factory CodeForgeLanguageServerRuntime({
    CodeForgeEnvironmentReader? environmentReader,
    bool? isWindows,
    CodeForgeExecutableExists? executableExists,
    CodeForgeLanguageServerTransportFactory? transportFactory,
    CodeForgeSupportDirectory? supportDirectory,
    ManagedLanguageServerInstallerPort? managedInstaller,
    LanguageIntelligenceActivityReporter? activityReporter,
    WorkspaceNestedCheckoutsPort? nestedCheckouts,
    Duration shutdownTimeout = const Duration(seconds: 5),
  }) => CodeForgeLanguageServerRuntime._(
    environmentReader: environmentReader ?? _platformEnvironment,
    isWindows: isWindows ?? Platform.isWindows,
    executableExists: executableExists,
    transportFactory: transportFactory ?? _startCodeForgeTransport,
    supportDirectory: supportDirectory ?? getApplicationSupportDirectory,
    managedInstaller:
        managedInstaller ??
        ManagedLanguageServerInstaller(
          environmentReader: environmentReader ?? _platformEnvironment,
          isWindows: isWindows ?? Platform.isWindows,
          executableExists: executableExists,
          activityReporter: activityReporter,
        ),
    nestedCheckouts: nestedCheckouts,
    shutdownTimeout: shutdownTimeout,
  );

  const CodeForgeLanguageServerRuntime._({
    required this._environmentReader,
    required this._isWindows,
    required this._executableExists,
    required this._transportFactory,
    required this._supportDirectory,
    required this._managedInstaller,
    required this._nestedCheckouts,
    required this._shutdownTimeout,
  });

  final CodeForgeEnvironmentReader _environmentReader;
  final bool _isWindows;
  final CodeForgeExecutableExists? _executableExists;
  final CodeForgeLanguageServerTransportFactory _transportFactory;
  final CodeForgeSupportDirectory _supportDirectory;
  final ManagedLanguageServerInstallerPort _managedInstaller;
  final WorkspaceNestedCheckoutsPort? _nestedCheckouts;

  /// A server busy indexing may never answer `shutdown`; the process is
  /// disposed regardless once this elapses.
  final Duration _shutdownTimeout;

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
          reason:
              '$reason No executable was otherwise found for ${provider.id}; '
              '$tried.',
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
    final initializationOptions = await _initializationOptionsFor(request);
    final configurationSections = await _configurationSectionsFor(request);

    final transport = await _transportFactory(
      CodeForgeLanguageServerLaunchSpec(
        executable: request.executable,
        workspaceRoot: request.workspaceRoot,
        bootstrapLanguageId: languageIds.first,
        arguments: request.arguments,
        environment: request.environment,
        initializationOptions: initializationOptions,
        capabilities: request.provider.capabilities,
      ),
    );
    final progressController = StreamController<LanguageServerWorkProgress>(
      sync: true,
    );
    final serverRequestSubscription = _listenForServerMessages(
      transport,
      workspaceRoot: request.workspaceRoot,
      isWindows: _isWindows,
      configurationSections: configurationSections,
      progressSink: progressController.sink,
    );
    try {
      await transport.initialize();
    } catch (_) {
      await serverRequestSubscription.cancel();
      unawaited(progressController.close());
      _disposeQuietly(transport);
      rethrow;
    }
    return CodeForgeLanguageServerSession._(
      transport,
      serverRequestSubscription,
      progressController,
    );
  }

  @override
  Future<void> clearWorkspaceStorage({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
  }) async {
    if (!_usesAleraManagedWorkspaceStorage(provider.id)) return;
    final directory = await _workspaceStorageDirectory(
      workspaceId: workspaceId,
      providerId: provider.id,
      create: false,
    );
    if (await directory.exists()) {
      await directory.delete(recursive: true);
    }
  }

  Future<Map<String, dynamic>> _initializationOptionsFor(
    LanguageServerRuntimeStartRequest request,
  ) async {
    if (!_usesAleraManagedWorkspaceStorage(request.provider.id)) {
      return const <String, dynamic>{};
    }
    final workspaceStorage = await _workspaceStorageDirectory(
      workspaceId: request.workspaceId,
      providerId: request.provider.id,
      create: true,
    );
    final globalStorage = await _globalStorageDirectory(
      providerId: request.provider.id,
      create: true,
    );
    return <String, dynamic>{
      'storagePath': workspaceStorage.path,
      'globalStoragePath': globalStorage.path,
    };
  }

  Future<Map<String, Object?>> _configurationSectionsFor(
    LanguageServerRuntimeStartRequest request,
  ) async {
    final locator = _nestedCheckouts;
    if (locator == null) return const <String, Object?>{};
    final List<String> nested;
    try {
      nested = await locator.nestedCheckouts(request.workspaceRoot);
    } catch (_) {
      // Not a git checkout, or git is unavailable: nothing to exclude.
      return const <String, Object?>{};
    }
    return nestedCheckoutExclusionSections(
      providerId: request.provider.id,
      workspaceRoot: request.workspaceRoot,
      nestedCheckouts: nested,
    );
  }

  Future<Directory> _workspaceStorageDirectory({
    required String workspaceId,
    required String providerId,
    required bool create,
  }) async {
    final support = await _supportDirectory();
    final directory = Directory(
      p.join(
        support.path,
        'language-index',
        'v1',
        'workspaces',
        _languageIndexSafeSegment(workspaceId),
        _languageIndexSafeSegment(providerId),
      ),
    );
    if (create) await directory.create(recursive: true);
    return directory;
  }

  Future<Directory> _globalStorageDirectory({
    required String providerId,
    required bool create,
  }) async {
    final support = await _supportDirectory();
    final directory = Directory(
      p.join(
        support.path,
        'language-index',
        'v1',
        'global',
        _languageIndexSafeSegment(providerId),
      ),
    );
    if (create) await directory.create(recursive: true);
    return directory;
  }

  @override
  Stream<LanguageServerExit> observeExit(LanguageServerRuntimeSession session) {
    final codeForgeSession = _requireSession(session);
    return _observeTransportExit(codeForgeSession.transport);
  }

  @override
  Stream<LanguageServerWorkProgress> observeProgress(
    LanguageServerRuntimeSession session,
  ) {
    final codeForgeSession = _requireSession(session);
    return codeForgeSession.progressController.stream;
  }

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {
    final codeForgeSession = _requireSession(session);
    final transport = codeForgeSession.transport;
    try {
      await transport.shutdown().timeout(_shutdownTimeout);
    } catch (_) {
      // A crashed, already-stopped or unresponsive server cannot answer
      // shutdown in time; dispose below still ends the process.
    }
    try {
      await transport.exitServer();
    } catch (_) {
      // Best-effort graceful exit; dispose below is the final process cleanup.
    }
    await codeForgeSession.serverRequestSubscription.cancel();
    unawaited(codeForgeSession.progressController.close());
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

  // Not an `async*` generator: cancelling a generator suspended at an `await`
  // completes only once that await resumes, and this one resumes when the
  // process exits. Stopping a session cancels this subscription before it
  // kills the process, so the cancel deadlocked and the server was never
  // stopped (Reindex Workspace spun forever and every restart leaked one).
  static Stream<LanguageServerExit> _observeTransportExit(
    CodeForgeLanguageServerTransport transport,
  ) => Stream<LanguageServerExit>.fromFuture(
    transport.processExitCode.then(
      (exitCode) => LanguageServerExit(exitCode: exitCode),
      onError: (Object error) => LanguageServerExit(error: error),
    ),
  );

  static void _disposeQuietly(CodeForgeLanguageServerTransport transport) {
    try {
      transport.dispose();
    } catch (_) {
      // Process cleanup is best effort after shutdown/init failure.
    }
  }

  static StreamSubscription<Map<String, dynamic>> _listenForServerMessages(
    CodeForgeLanguageServerTransport transport, {
    required String workspaceRoot,
    required bool isWindows,
    required Map<String, Object?> configurationSections,
    required StreamSink<LanguageServerWorkProgress> progressSink,
  }) => transport.responses.listen((message) {
    final id = message['id'];
    final method = message['method'];
    if (method == r'$/progress') {
      final progress = _decodeWorkProgress(message);
      if (progress != null) {
        progressSink.add(progress);
      }
      return;
    }
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
        configurationSections: configurationSections,
      ),
    );
  });

  static LanguageServerWorkProgress? _decodeWorkProgress(
    Map<String, dynamic> message,
  ) {
    final params = message['params'];
    if (params is! Map) return null;
    final token = params['token'];
    final value = params['value'];
    if (token == null || value is! Map) return null;
    final kind = value['kind']?.toString();
    if (kind != 'begin' && kind != 'report' && kind != 'end') return null;
    final rawPercentage = value['percentage'];
    final percentage = rawPercentage is num
        ? rawPercentage.toDouble().clamp(0.0, 100.0)
        : null;
    return LanguageServerWorkProgress(
      token: token.toString(),
      title: value['title']?.toString(),
      message: value['message']?.toString(),
      percentage: percentage,
      done: kind == 'end',
    );
  }

  static Future<void> _respondToServerRequest(
    CodeForgeLanguageServerTransport transport, {
    required Object id,
    required String method,
    required Object? params,
    required String workspaceRoot,
    required bool isWindows,
    required Map<String, Object?> configurationSections,
  }) async {
    try {
      switch (method) {
        case 'workspace/configuration':
          final items = params is Map ? params['items'] : null;
          await transport.sendResponse(
            id: id,
            result: <Object?>[
              if (items is List)
                for (final item in items)
                  configurationSections[item is Map ? item['section'] : null] ??
                      <String, dynamic>{},
            ],
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
    // `.cmd` servers start through `cmd.exe`, which cannot use a verbatim
    // working directory.
    workspacePath: withoutWindowsPathPrefix(spec.workspaceRoot),
    languageId: spec.bootstrapLanguageId,
    args: spec.arguments,
    environment: spec.environment,
    initializationOptions: spec.initializationOptions,
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

bool _usesAleraManagedWorkspaceStorage(String providerId) =>
    providerId == 'php.intelephense';

String _languageIndexSafeSegment(String value) =>
    value.replaceAll(RegExp(r'[^A-Za-z0-9._+-]'), '_');
