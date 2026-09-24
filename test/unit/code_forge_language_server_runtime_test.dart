import 'dart:async';
import 'dart:io';

import 'package:path/path.dart' as p;

import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/application/managed_language_server_installer.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_language_server_runtime.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late LanguageProviderDescriptor provider;

  setUp(() {
    provider = LanguageProviderDescriptor(
      id: 'rust-semantic',
      kind: LanguageProviderKind.semanticServer,
      languages: <LanguageId>{LanguageId('rust')},
      capabilities: const <LanguageCapability>{
        LanguageCapability.definition,
        LanguageCapability.references,
        LanguageCapability.hover,
      },
      processScope: LanguageProviderProcessScope.workspace,
      launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
      executableResolutionPolicy:
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
      executableCandidates: const <String>[
        'rust-analyzer',
        'rust-analyzer-alt',
      ],
      defaultArguments: const <String>['--stdio'],
    );
  });

  test('explicit executable override wins over PATH candidates', () async {
    final runtime = CodeForgeLanguageServerRuntime(
      environmentReader: () => const <String, String>{
        'Path': r'C:\bin',
        'PATHEXT': '.EXE;.CMD',
      },
      isWindows: true,
      executableExists: (path) => path == r'C:\tools\rust-analyzer.exe',
      transportFactory: _unexpectedTransportFactory,
    );

    final result = await runtime.resolveExecutable(
      provider: provider,
      settings: const LanguageActivationSettings(
        enabled: true,
        executablePath: r'C:\tools\rust-analyzer.exe',
      ),
      target: LanguageServerTarget.localWorkspace,
    );

    expect(result, isA<LanguageServerExecutableResolved>());
    expect(
      (result as LanguageServerExecutableResolved).executable,
      r'C:\tools\rust-analyzer.exe',
    );
  });

  test('PATH resolution honors provider candidate order and PATHEXT', () async {
    final checkedPaths = <String>[];
    final runtime = CodeForgeLanguageServerRuntime(
      environmentReader: () => const <String, String>{
        'Path': r'C:\first;C:\second',
        'PATHEXT': '.EXE;.CMD',
      },
      isWindows: true,
      executableExists: (path) {
        checkedPaths.add(path);
        return path.toLowerCase() == r'c:\second\rust-analyzer-alt.exe';
      },
      transportFactory: _unexpectedTransportFactory,
    );

    final result = await runtime.resolveExecutable(
      provider: provider,
      settings: const LanguageActivationSettings(enabled: true),
      target: LanguageServerTarget.localWorkspace,
    );

    expect(result, isA<LanguageServerExecutableResolved>());
    expect(
      (result as LanguageServerExecutableResolved).executable,
      r'C:\second\rust-analyzer-alt.exe',
    );
    expect(
      checkedPaths.any(
        (path) => path.toLowerCase() == r'c:\second\rust-analyzer-alt.exe',
      ),
      isTrue,
    );
    expect(checkedPaths, isNot(contains('rust-analyzer')));
  });

  test('bare executable override resolves to its concrete PATH shim', () async {
    final runtime = CodeForgeLanguageServerRuntime(
      environmentReader: () => const <String, String>{
        'Path': r'C:\node\bin',
        'PATHEXT': '.EXE;.CMD',
      },
      isWindows: true,
      executableExists: (path) =>
          path.toLowerCase() == r'c:\node\bin\pyright-langserver.cmd',
      transportFactory: _unexpectedTransportFactory,
    );

    final result = await runtime.resolveExecutable(
      provider: provider,
      settings: const LanguageActivationSettings(
        enabled: true,
        executablePath: 'pyright-langserver',
      ),
      target: LanguageServerTarget.localWorkspace,
    );

    expect(result, isA<LanguageServerExecutableResolved>());
    expect(
      (result as LanguageServerExecutableResolved).executable,
      r'C:\node\bin\pyright-langserver.cmd',
    );
  });

  test('PATH miss falls back to the managed language server', () async {
    final managed = _FakeManagedInstaller(
      result: const ManagedLanguageServerInstalled(
        executable: r'C:\Alera\language-servers\rust-analyzer.exe',
        version: '1.97.1',
      ),
    );
    final runtime = CodeForgeLanguageServerRuntime(
      environmentReader: () => const <String, String>{
        'Path': r'C:\empty',
        'PATHEXT': '.EXE;.CMD',
      },
      isWindows: true,
      executableExists: (path) =>
          path == r'C:\Alera\language-servers\rust-analyzer.exe',
      managedInstaller: managed,
      transportFactory: _unexpectedTransportFactory,
    );

    final result = await runtime.resolveExecutable(
      provider: provider,
      settings: const LanguageActivationSettings(enabled: true),
      target: LanguageServerTarget.localWorkspace,
    );

    expect(result, isA<LanguageServerExecutableResolved>());
    expect(
      (result as LanguageServerExecutableResolved).executable,
      r'C:\Alera\language-servers\rust-analyzer.exe',
    );
    expect(managed.calls, 1);
  });

  test(
    'invalid explicit override does not auto-install a replacement',
    () async {
      final managed = _FakeManagedInstaller(
        result: const ManagedLanguageServerInstalled(
          executable: r'C:\Alera\language-servers\rust-analyzer.exe',
          version: '1.97.1',
        ),
      );
      final runtime = CodeForgeLanguageServerRuntime(
        environmentReader: () => const <String, String>{
          'Path': r'C:\empty',
          'PATHEXT': '.EXE;.CMD',
        },
        isWindows: true,
        executableExists: (_) => false,
        managedInstaller: managed,
        transportFactory: _unexpectedTransportFactory,
      );

      final result = await runtime.resolveExecutable(
        provider: provider,
        settings: const LanguageActivationSettings(
          enabled: true,
          executablePath: r'C:\missing\rust-analyzer.exe',
        ),
        target: LanguageServerTarget.localWorkspace,
      );

      expect(result, isA<LanguageServerExecutableMissing>());
      expect(managed.calls, 0);
    },
  );

  test('provider candidates reject repository-relative executable paths', () {
    expect(
      () => LanguageProviderDescriptor(
        id: 'unsafe',
        kind: LanguageProviderKind.semanticServer,
        languages: <LanguageId>{LanguageId('rust')},
        capabilities: const <LanguageCapability>{LanguageCapability.definition},
        processScope: LanguageProviderProcessScope.workspace,
        launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
        executableResolutionPolicy:
            LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
        executableCandidates: const <String>[r'.\tools\server.exe'],
      ),
      throwsArgumentError,
    );
  });

  test(
    'remote workspace resolution is explicit and never falls back local',
    () async {
      var filesystemChecks = 0;
      final runtime = CodeForgeLanguageServerRuntime(
        environmentReader: () => const <String, String>{'PATH': '/usr/bin'},
        isWindows: false,
        executableExists: (_) {
          filesystemChecks += 1;
          return true;
        },
        transportFactory: _unexpectedTransportFactory,
      );

      final result = await runtime.resolveExecutable(
        provider: provider,
        settings: const LanguageActivationSettings(enabled: true),
        target: LanguageServerTarget.remoteWorkspace,
      );

      expect(result, isA<LanguageServerExecutableMissing>());
      expect(
        (result as LanguageServerExecutableMissing).reason,
        contains('Remote workspace'),
      );
      expect(filesystemChecks, 0);
    },
  );

  test(
    'start initializes transport and preserves provider launch metadata',
    () async {
      final transport = _FakeTransport();
      CodeForgeLanguageServerLaunchSpec? captured;
      final runtime = CodeForgeLanguageServerRuntime(
        transportFactory: (spec) async {
          captured = spec;
          return transport;
        },
      );

      final session = await runtime.start(
        LanguageServerRuntimeStartRequest(
          provider: provider,
          executable: 'rust-analyzer',
          workspaceRoot: r'C:\repo',
          target: LanguageServerTarget.localWorkspace,
          arguments: const <String>['--stdio', '--log-file', 'ra.log'],
          environment: const <String, String>{'RUST_LOG': 'info'},
        ),
      );

      expect(session, isA<CodeForgeLanguageServerSession>());
      expect(transport.initializeCalls, 1);
      expect(captured!.executable, 'rust-analyzer');
      expect(captured!.workspaceRoot, r'C:\repo');
      expect(captured!.bootstrapLanguageId, 'rust');
      expect(captured!.arguments, <String>['--stdio', '--log-file', 'ra.log']);
      expect(captured!.capabilities, provider.capabilities);
    },
  );

  test('server configuration requests are answered while initialization is in progress', () async {
    final transport = _FakeTransport(
      initializationRequest: <String, dynamic>{
        'jsonrpc': '2.0',
        'id': 'config-1',
        'method': 'workspace/configuration',
        'params': <String, dynamic>{
          'items': <Map<String, dynamic>>[
            <String, dynamic>{'section': 'gopls'},
            <String, dynamic>{'section': 'gopls.ui'},
          ],
        },
      },
    );
    final runtime = CodeForgeLanguageServerRuntime(
      transportFactory: (_) async => transport,
    );

    final session = await runtime.start(
      LanguageServerRuntimeStartRequest(
        provider: provider,
        executable: 'gopls',
        workspaceRoot: r'C:\repo',
        target: LanguageServerTarget.localWorkspace,
      ),
    );

    expect(transport.sentResponses, <Map<String, dynamic>>[
      <String, dynamic>{
        'id': 'config-1',
        'result': <Map<String, dynamic>>[
          <String, dynamic>{},
          <String, dynamic>{},
        ],
      },
    ]);
    await runtime.stop(session);
  });

  test('unknown server requests receive method-not-found responses', () async {
    final transport = _FakeTransport();
    final runtime = CodeForgeLanguageServerRuntime(
      transportFactory: (_) async => transport,
    );
    final session = await runtime.start(
      LanguageServerRuntimeStartRequest(
        provider: provider,
        executable: 'gopls',
        workspaceRoot: r'C:\repo',
        target: LanguageServerTarget.localWorkspace,
      ),
    );

    transport.emit(<String, dynamic>{
      'jsonrpc': '2.0',
      'id': 42,
      'method': 'custom/unsupportedRequest',
      'params': <String, dynamic>{},
    });
    await Future<void>.delayed(Duration.zero);

    expect(transport.sentErrors, <Map<String, dynamic>>[
      <String, dynamic>{
        'id': 42,
        'code': -32601,
        'message':
            'Unsupported language-server request: custom/unsupportedRequest',
        'data': null,
      },
    ]);
    await runtime.stop(session);
  });

  test('LSP work-done progress notifications are exposed in order', () async {
    final transport = _FakeTransport();
    final runtime = CodeForgeLanguageServerRuntime(
      transportFactory: (_) async => transport,
    );
    final session = await runtime.start(
      LanguageServerRuntimeStartRequest(
        provider: provider,
        executable: 'rust-analyzer',
        workspaceRoot: r'C:\repo',
        target: LanguageServerTarget.localWorkspace,
      ),
    );
    final progressFuture = runtime.observeProgress(session).take(3).toList();

    transport.emit(<String, dynamic>{
      'jsonrpc': '2.0',
      'method': r'$/progress',
      'params': <String, dynamic>{
        'token': 'index',
        'value': <String, dynamic>{
          'kind': 'begin',
          'title': 'Indexing',
          'percentage': 10,
        },
      },
    });
    transport.emit(<String, dynamic>{
      'jsonrpc': '2.0',
      'method': r'$/progress',
      'params': <String, dynamic>{
        'token': 'index',
        'value': <String, dynamic>{
          'kind': 'report',
          'message': 'Scanning crates',
          'percentage': 55,
        },
      },
    });
    transport.emit(<String, dynamic>{
      'jsonrpc': '2.0',
      'method': r'$/progress',
      'params': <String, dynamic>{
        'token': 'index',
        'value': <String, dynamic>{'kind': 'end', 'message': 'Done'},
      },
    });

    final progress = await progressFuture;
    expect(progress, hasLength(3));
    expect(progress[0].token, 'index');
    expect(progress[0].title, 'Indexing');
    expect(progress[0].percentage, 10);
    expect(progress[0].done, isFalse);
    expect(progress[1].message, 'Scanning crates');
    expect(progress[1].percentage, 55);
    expect(progress[2].done, isTrue);

    await runtime.stop(session);
  });

  test(
    'progress emitted during initialize is buffered until observed',
    () async {
      final transport = _FakeTransport(
        initializationNotification: <String, dynamic>{
          'jsonrpc': '2.0',
          'method': r'$/progress',
          'params': <String, dynamic>{
            'token': 'startup-index',
            'value': <String, dynamic>{
              'kind': 'begin',
              'title': 'Loading workspace',
              'percentage': 5,
            },
          },
        },
      );
      final runtime = CodeForgeLanguageServerRuntime(
        transportFactory: (_) async => transport,
      );
      final session = await runtime.start(
        LanguageServerRuntimeStartRequest(
          provider: provider,
          executable: 'rust-analyzer',
          workspaceRoot: r'C:\repo',
          target: LanguageServerTarget.localWorkspace,
        ),
      );

      final progress = await runtime.observeProgress(session).first;
      expect(progress.token, 'startup-index');
      expect(progress.title, 'Loading workspace');
      expect(progress.percentage, 5);
      await runtime.stop(session);
    },
  );

  test(
    'initialization failure disposes transport and does not return a session',
    () async {
      final transport = _FakeTransport(initializeError: StateError('bad init'));
      final runtime = CodeForgeLanguageServerRuntime(
        transportFactory: (_) async => transport,
      );

      await expectLater(
        runtime.start(
          LanguageServerRuntimeStartRequest(
            provider: provider,
            executable: 'rust-analyzer',
            workspaceRoot: r'C:\repo',
            target: LanguageServerTarget.localWorkspace,
          ),
        ),
        throwsStateError,
      );
      expect(transport.disposeCalls, 1);
    },
  );

  test('Intelephense receives persistent Alera storage paths and workspace cache can be cleared', () async {
    final support = Directory(
      p.join('build', 'test-language-index', 'intelephense-storage'),
    );
    if (await support.exists()) {
      await support.delete(recursive: true);
    }
    await support.create(recursive: true);
    addTearDown(() async {
      if (await support.exists()) {
        await support.delete(recursive: true);
      }
    });
    final transport = _FakeTransport();
    CodeForgeLanguageServerLaunchSpec? captured;
    final runtime = CodeForgeLanguageServerRuntime(
      supportDirectory: () async => support,
      transportFactory: (spec) async {
        captured = spec;
        return transport;
      },
    );
    final phpProvider = LanguageProviderDescriptor(
      id: 'php.intelephense',
      kind: LanguageProviderKind.semanticServer,
      languages: <LanguageId>{LanguageId('php')},
      capabilities: const <LanguageCapability>{LanguageCapability.definition},
      processScope: LanguageProviderProcessScope.workspace,
      launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
      executableResolutionPolicy:
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
      executableCandidates: const <String>['intelephense'],
      defaultArguments: const <String>['--stdio'],
    );

    final session = await runtime.start(
      LanguageServerRuntimeStartRequest(
        workspaceId: 'workspace-a',
        provider: phpProvider,
        executable: 'intelephense',
        workspaceRoot: 'repo-root',
        target: LanguageServerTarget.localWorkspace,
      ),
    );

    final workspaceStorage = p.join(
      support.path,
      'language-index',
      'v1',
      'workspaces',
      'workspace-a',
      'php.intelephense',
    );
    final globalStorage = p.join(
      support.path,
      'language-index',
      'v1',
      'global',
      'php.intelephense',
    );
    expect(captured!.initializationOptions, <String, dynamic>{
      'storagePath': workspaceStorage,
      'globalStoragePath': globalStorage,
    });
    expect(await Directory(workspaceStorage).exists(), isTrue);
    expect(await Directory(globalStorage).exists(), isTrue);
    await File(p.join(workspaceStorage, 'index.marker')).writeAsString('x');

    await runtime.clearWorkspaceStorage(
      workspaceId: 'workspace-a',
      provider: phpProvider,
    );

    expect(await Directory(workspaceStorage).exists(), isFalse);
    expect(await Directory(globalStorage).exists(), isTrue);
    await runtime.stop(session);
  });

  test(
    'stop is graceful and observeExit normalizes the process result',
    () async {
      final transport = _FakeTransport();
      final runtime = CodeForgeLanguageServerRuntime(
        transportFactory: (_) async => transport,
      );
      final session = await runtime.start(
        LanguageServerRuntimeStartRequest(
          provider: provider,
          executable: 'rust-analyzer',
          workspaceRoot: r'C:\repo',
          target: LanguageServerTarget.localWorkspace,
        ),
      );
      final exitFuture = runtime.observeExit(session).first;

      transport.exitCode.complete(23);
      final exit = await exitFuture;
      expect(exit.exitCode, 23);

      await runtime.stop(session);
      expect(transport.shutdownCalls, 1);
      expect(transport.exitCalls, 1);
      expect(transport.disposeCalls, 1);
    },
  );
}

final class _FakeManagedInstaller
    implements ManagedLanguageServerInstallerPort {
  _FakeManagedInstaller({required this.result});

  final ManagedLanguageServerInstallResult result;
  int calls = 0;

  @override
  Future<ManagedLanguageServerInstallResult> ensureInstalled(
    LanguageProviderDescriptor provider,
  ) async {
    calls += 1;
    return result;
  }
}

Future<CodeForgeLanguageServerTransport> _unexpectedTransportFactory(
  CodeForgeLanguageServerLaunchSpec _,
) => throw StateError('transport must not be started');

final class _FakeTransport implements CodeForgeLanguageServerTransport {
  _FakeTransport({
    this.initializeError,
    this.initializationRequest,
    this.initializationNotification,
  });

  final Object? initializeError;
  final Map<String, dynamic>? initializationRequest;
  final Map<String, dynamic>? initializationNotification;
  final Completer<int> exitCode = Completer<int>();
  final StreamController<Map<String, dynamic>> _responses =
      StreamController<Map<String, dynamic>>.broadcast();
  final List<Map<String, dynamic>> sentResponses = <Map<String, dynamic>>[];
  final List<Map<String, dynamic>> sentErrors = <Map<String, dynamic>>[];
  Completer<void>? _initializationReply;
  int initializeCalls = 0;
  int shutdownCalls = 0;
  int exitCalls = 0;
  int disposeCalls = 0;

  @override
  Future<int> get processExitCode => exitCode.future;

  @override
  Future<void> initialize() async {
    initializeCalls += 1;
    final error = initializeError;
    if (error != null) throw error;
    final notification = initializationNotification;
    if (notification != null) {
      _responses.add(notification);
    }
    final request = initializationRequest;
    if (request != null) {
      _initializationReply = Completer<void>();
      _responses.add(request);
      await _initializationReply!.future;
    }
  }

  @override
  Future<void> shutdown() async {
    shutdownCalls += 1;
  }

  @override
  Future<void> exitServer() async {
    exitCalls += 1;
  }

  @override
  void dispose() {
    disposeCalls += 1;
    unawaited(_responses.close());
    if (!exitCode.isCompleted) exitCode.complete(0);
  }

  @override
  Future<Map<String, dynamic>> sendRequest({
    required String method,
    required Map<String, dynamic> params,
  }) => throw UnimplementedError();

  @override
  Future<void> sendNotification({
    required String method,
    required Map<String, dynamic> params,
  }) => throw UnimplementedError();

  @override
  Future<void> sendResponse({required Object id, Object? result}) async {
    sentResponses.add(<String, dynamic>{'id': id, 'result': result});
    final reply = _initializationReply;
    if (reply != null && !reply.isCompleted) {
      reply.complete();
    }
  }

  @override
  Future<void> sendErrorResponse({
    required Object id,
    required int code,
    required String message,
    Object? data,
  }) async {
    sentErrors.add(<String, dynamic>{
      'id': id,
      'code': code,
      'message': message,
      'data': data,
    });
    final reply = _initializationReply;
    if (reply != null && !reply.isCompleted) {
      reply.complete();
    }
  }

  void emit(Map<String, dynamic> message) => _responses.add(message);

  @override
  Stream<Map<String, dynamic>> get responses => _responses.stream;
}
