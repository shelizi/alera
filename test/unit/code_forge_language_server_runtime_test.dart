import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
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
      'rust-analyzer-alt',
    );
    expect(
      checkedPaths.any(
        (path) => path.toLowerCase() == r'c:\second\rust-analyzer-alt.exe',
      ),
      isTrue,
    );
    expect(checkedPaths, isNot(contains('rust-analyzer')));
  });

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

Future<CodeForgeLanguageServerTransport> _unexpectedTransportFactory(
  CodeForgeLanguageServerLaunchSpec _,
) => throw StateError('transport must not be started');

final class _FakeTransport implements CodeForgeLanguageServerTransport {
  _FakeTransport({this.initializeError});

  final Object? initializeError;
  final Completer<int> exitCode = Completer<int>();
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
  Stream<Map<String, dynamic>> get responses => const Stream.empty();
}
