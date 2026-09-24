import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_intelligence_activity.dart';
import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_session_manager.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_server_session_state.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late LanguageId rust;
  late LanguageExtensionRegistry registry;
  late _FakeLanguageServerRuntime runtime;

  setUp(() {
    rust = LanguageId('rust');
    registry = _registry(rust);
    runtime = _FakeLanguageServerRuntime();
  });

  test(
    'support registration and disabled settings never start a process',
    () async {
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
      );

      expect(runtime.resolveCalls, 0);
      expect(runtime.startCalls, 0);

      final snapshot = await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: r'C:\repo\src\main.rs',
        settings: LanguageIntelligenceSettings.defaults,
        target: LanguageServerTarget.localWorkspace,
      );

      expect(snapshot.state, LanguageServerSessionState.disabled);
      expect(runtime.resolveCalls, 0);
      expect(runtime.startCalls, 0);
    },
  );

  test(
    'workspace reindex clears managed storage and restarts prewarmed server',
    () async {
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
      );
      final settings = _enabledSettings(rust);

      await manager.prewarmLanguage(
        workspaceId: 'workspace-a',
        workspaceRoot: 'repo-root',
        language: rust,
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );

      final restarted = await manager.reindexWorkspace('workspace-a');

      expect(restarted, 1);
      expect(runtime.stopCalls, 1);
      expect(runtime.clearStorageCalls, 1);
      expect(runtime.clearedWorkspaces, <String>['workspace-a']);
      expect(runtime.clearedProviders, <String>['rust-semantic']);
      expect(runtime.startCalls, 2);
      expect(runtime.startRequests.last.workspaceId, 'workspace-a');
      expect(
        manager.snapshotFor('workspace-a', 'rust-semantic').state,
        LanguageServerSessionState.ready,
      );
    },
  );

  test('enabled documents share one lazy workspace provider session', () async {
    final manager = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
    );
    final settings = _enabledSettings(rust);

    final first = await manager.attachDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      language: rust,
      documentId: r'C:\repo\src\main.rs',
      settings: settings,
      target: LanguageServerTarget.localWorkspace,
    );
    final second = await manager.attachDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      language: rust,
      documentId: r'C:\repo\src\lib.rs',
      settings: settings,
      target: LanguageServerTarget.localWorkspace,
    );

    expect(first.state, LanguageServerSessionState.ready);
    expect(second.state, LanguageServerSessionState.ready);
    expect(runtime.resolveCalls, 1);
    expect(runtime.startCalls, 1);
    expect(runtime.startRequests.single.arguments, <String>[
      '--stdio',
      '--trace',
    ]);
    expect(second.activeDocumentCount, 2);
    expect(
      manager.snapshotFor('workspace-a', 'rust-semantic').activeDocumentCount,
      2,
    );
  });

  test(
    'workspace prewarm keeps a zero-document session alive until lease release',
    () async {
      final delay = _ControlledDelay();
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
        idleShutdownDelay: const Duration(seconds: 30),
        delay: delay.call,
      );
      final settings = _enabledSettings(rust);

      final prewarmed = await manager.prewarmLanguage(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );

      expect(prewarmed.state, LanguageServerSessionState.ready);
      expect(prewarmed.activeDocumentCount, 0);
      expect(runtime.startCalls, 1);

      await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: 'main.rs',
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      expect(runtime.startCalls, 1);

      await manager.releaseDocument(
        workspaceId: 'workspace-a',
        providerId: 'rust-semantic',
        documentId: 'main.rs',
      );
      expect(delay.pending, 0);

      manager.releaseWorkspacePrewarm('workspace-a');
      expect(delay.pending, 1);
      delay.completeNext();
      await _flushAsync();

      expect(runtime.stopCalls, 1);
      expect(
        manager.snapshotFor('workspace-a', 'rust-semantic').state,
        LanguageServerSessionState.available,
      );
    },
  );

  test(
    'enabled language falls back to its declared default semantic provider',
    () async {
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
      );

      final snapshot = await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: r'C:\repo\src\main.rs',
        settings: LanguageIntelligenceSettings().withLanguage(
          rust,
          const LanguageActivationSettings(enabled: true),
        ),
        target: LanguageServerTarget.localWorkspace,
      );

      expect(snapshot.state, LanguageServerSessionState.ready);
      expect(snapshot.providerId, 'rust-semantic');
      expect(runtime.startCalls, 1);
    },
  );

  test(
    'last document schedules idle stop and new activity cancels it',
    () async {
      final delay = _ControlledDelay();
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
        idleShutdownDelay: const Duration(seconds: 30),
        delay: delay.call,
      );
      final settings = _enabledSettings(rust);

      await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: 'main.rs',
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      await manager.releaseDocument(
        workspaceId: 'workspace-a',
        providerId: 'rust-semantic',
        documentId: 'main.rs',
      );
      expect(delay.pending, 1);

      await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: 'lib.rs',
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      delay.completeNext();
      await _flushAsync();
      expect(runtime.stopCalls, 0);

      await manager.releaseDocument(
        workspaceId: 'workspace-a',
        providerId: 'rust-semantic',
        documentId: 'lib.rs',
      );
      delay.completeNext();
      await _flushAsync();

      expect(runtime.stopCalls, 1);
      expect(
        manager.snapshotFor('workspace-a', 'rust-semantic').state,
        LanguageServerSessionState.available,
      );
    },
  );

  test('settings detach stops immediately only after the shared provider is unused', () async {
    final delay = _ControlledDelay();
    final manager = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
      idleShutdownDelay: const Duration(seconds: 30),
      delay: delay.call,
    );
    final settings = _enabledSettings(rust);
    for (final document in <String>['main.rs', 'lib.rs']) {
      await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: document,
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
    }

    await manager.releaseDocument(
      workspaceId: 'workspace-a',
      providerId: 'rust-semantic',
      documentId: 'main.rs',
      stopWhenUnused: true,
    );
    expect(runtime.stopCalls, 0);

    await manager.releaseDocument(
      workspaceId: 'workspace-a',
      providerId: 'rust-semantic',
      documentId: 'lib.rs',
      stopWhenUnused: true,
    );

    expect(runtime.stopCalls, 1);
    expect(delay.pending, 0);
    expect(
      manager.snapshotFor('workspace-a', 'rust-semantic').state,
      LanguageServerSessionState.available,
    );
  });

  test('missing executable is explicit and never calls start', () async {
    runtime.resolution = const LanguageServerExecutableMissing(
      reason: 'rust-analyzer was not found',
    );
    final manager = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
    );

    final snapshot = await manager.attachDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      language: rust,
      documentId: 'main.rs',
      settings: _enabledSettings(rust),
      target: LanguageServerTarget.localWorkspace,
    );

    expect(snapshot.state, LanguageServerSessionState.missingExecutable);
    expect(snapshot.lastError, contains('rust-analyzer'));
    expect(runtime.resolveCalls, 1);
    expect(runtime.startCalls, 0);
  });

  test(
    'disable during start stops the stale process instead of publishing it',
    () async {
      final startCompleter = Completer<LanguageServerRuntimeSession>();
      runtime.startCompleter = startCompleter;
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
      );

      final attachFuture = manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: 'main.rs',
        settings: _enabledSettings(rust),
        target: LanguageServerTarget.localWorkspace,
      );
      await _flushAsync();
      expect(runtime.startCalls, 1);

      await manager.disableProvider(
        workspaceId: 'workspace-a',
        providerId: 'rust-semantic',
      );
      final staleSession = _FakeRuntimeSession('stale');
      startCompleter.complete(staleSession);
      await attachFuture;
      await _flushAsync();

      expect(runtime.stoppedSessions, contains(staleSession));
      expect(
        manager.snapshotFor('workspace-a', 'rust-semantic').state,
        LanguageServerSessionState.disabled,
      );
    },
  );

  test('provider crash performs only bounded automatic restarts', () async {
    final delay = _ControlledDelay();
    final manager = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
      restartBackoff: const Duration(seconds: 1),
      maxRestartAttempts: 1,
      delay: delay.call,
    );

    await manager.attachDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      language: rust,
      documentId: 'main.rs',
      settings: _enabledSettings(rust),
      target: LanguageServerTarget.localWorkspace,
    );
    expect(runtime.startCalls, 1);

    runtime.crashLatest(exitCode: 17);
    await _flushAsync();
    expect(
      manager.snapshotFor('workspace-a', 'rust-semantic').state,
      LanguageServerSessionState.failed,
    );
    expect(delay.pending, 1);

    delay.completeNext();
    await _flushAsync();
    expect(runtime.startCalls, 2);
    expect(
      manager.snapshotFor('workspace-a', 'rust-semantic').state,
      LanguageServerSessionState.ready,
    );

    runtime.crashLatest(exitCode: 18);
    await _flushAsync();
    expect(delay.pending, 0);
    expect(runtime.startCalls, 2);
    expect(
      manager.snapshotFor('workspace-a', 'rust-semantic').state,
      LanguageServerSessionState.failed,
    );
  });

  test(
    'reports LSP progress and manual restart starts a fresh session',
    () async {
      final activity = LanguageIntelligenceActivityStore();
      addTearDown(activity.dispose);
      final manager = LanguageServerSessionManager(
        registry: registry,
        runtime: runtime,
        activityReporter: activity,
      );

      await manager.attachDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        language: rust,
        documentId: 'main.rs',
        settings: _enabledSettings(rust),
        target: LanguageServerTarget.localWorkspace,
      );

      runtime.emitProgressLatest(
        const LanguageServerWorkProgress(
          token: 'index',
          title: 'Indexing',
          message: 'Scanning crates',
          percentage: 40,
        ),
      );
      await _flushAsync();
      final progress = activity.snapshot.progressForWorkspace('workspace-a');
      expect(progress, hasLength(1));
      expect(progress.single.providerId, 'rust-semantic');
      expect(progress.single.title, 'Indexing');
      expect(progress.single.percentage, 40);

      runtime.emitProgressLatest(
        const LanguageServerWorkProgress(token: 'index', done: true),
      );
      await _flushAsync();
      expect(activity.snapshot.progressForWorkspace('workspace-a'), isEmpty);

      expect(
        await manager.restartProvider(
          workspaceId: 'workspace-a',
          providerId: 'rust-semantic',
        ),
        isTrue,
      );
      expect(runtime.stopCalls, 1);
      expect(runtime.startCalls, 2);
      expect(
        manager.snapshotFor('workspace-a', 'rust-semantic').state,
        LanguageServerSessionState.ready,
      );
    },
  );
}

LanguageExtensionRegistry _registry(LanguageId rust) {
  final registry = LanguageExtensionRegistry();
  registry.registerLanguage(
    LanguageExtensionDescriptor(
      id: rust,
      displayName: 'Rust',
      fileExtensions: const <String>['rs'],
      semanticProviderIds: const <String>['rust-semantic'],
      defaultSemanticProviderId: 'rust-semantic',
      capabilities: const <LanguageCapability>{
        LanguageCapability.definition,
        LanguageCapability.references,
      },
    ),
  );
  registry.registerProvider(
    LanguageProviderDescriptor(
      id: 'rust-semantic',
      kind: LanguageProviderKind.semanticServer,
      languages: <LanguageId>{rust},
      capabilities: const <LanguageCapability>{
        LanguageCapability.definition,
        LanguageCapability.references,
      },
      processScope: LanguageProviderProcessScope.workspace,
      launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
      executableResolutionPolicy:
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
      executableCandidates: const <String>['rust-analyzer'],
      defaultArguments: const <String>['--stdio'],
    ),
  );
  return registry;
}

LanguageIntelligenceSettings _enabledSettings(LanguageId language) =>
    LanguageIntelligenceSettings().withLanguage(
      language,
      const LanguageActivationSettings(
        enabled: true,
        semanticProviderId: 'rust-semantic',
        extraArgs: <String>['--trace'],
      ),
    );

Future<void> _flushAsync() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

final class _FakeRuntimeSession implements LanguageServerRuntimeSession {
  const _FakeRuntimeSession(this.id);

  final String id;
}

final class _FakeLanguageServerRuntime
    implements
        LanguageServerRuntimePort,
        LanguageServerProgressRuntimePort,
        LanguageServerWorkspaceStorageRuntimePort {
  LanguageServerExecutableResolution resolution =
      const LanguageServerExecutableResolved(r'C:\tools\server.exe');
  Completer<LanguageServerRuntimeSession>? startCompleter;
  int resolveCalls = 0;
  int startCalls = 0;
  int stopCalls = 0;
  int clearStorageCalls = 0;
  final List<String> clearedWorkspaces = <String>[];
  final List<String> clearedProviders = <String>[];
  final List<LanguageServerRuntimeSession> stoppedSessions =
      <LanguageServerRuntimeSession>[];
  final List<_FakeRuntimeSession> sessions = <_FakeRuntimeSession>[];
  final List<LanguageServerRuntimeStartRequest> startRequests =
      <LanguageServerRuntimeStartRequest>[];
  final Map<LanguageServerRuntimeSession, StreamController<LanguageServerExit>>
  exits =
      <LanguageServerRuntimeSession, StreamController<LanguageServerExit>>{};
  final Map<
    LanguageServerRuntimeSession,
    StreamController<LanguageServerWorkProgress>
  >
  progress =
      <
        LanguageServerRuntimeSession,
        StreamController<LanguageServerWorkProgress>
      >{};

  @override
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  }) async {
    resolveCalls += 1;
    return resolution;
  }

  @override
  Future<void> clearWorkspaceStorage({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
  }) async {
    clearStorageCalls += 1;
    clearedWorkspaces.add(workspaceId);
    clearedProviders.add(provider.id);
  }

  @override
  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  ) async {
    startCalls += 1;
    startRequests.add(request);
    final completer = startCompleter;
    final LanguageServerRuntimeSession session;
    if (completer != null) {
      startCompleter = null;
      session = await completer.future;
    } else {
      session = _FakeRuntimeSession('session-$startCalls');
    }
    if (session is _FakeRuntimeSession) {
      sessions.add(session);
    }
    exits[session] = StreamController<LanguageServerExit>.broadcast();
    progress[session] = StreamController<LanguageServerWorkProgress>.broadcast(
      sync: true,
    );
    return session;
  }

  @override
  Stream<LanguageServerExit> observeExit(
    LanguageServerRuntimeSession session,
  ) => exits[session]!.stream;

  @override
  Stream<LanguageServerWorkProgress> observeProgress(
    LanguageServerRuntimeSession session,
  ) => progress[session]!.stream;

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {
    stopCalls += 1;
    stoppedSessions.add(session);
    await exits[session]?.close();
    await progress[session]?.close();
  }

  void crashLatest({required int exitCode}) {
    final session = sessions.last;
    exits[session]!.add(LanguageServerExit(exitCode: exitCode));
  }

  void emitProgressLatest(LanguageServerWorkProgress event) {
    progress[sessions.last]!.add(event);
  }
}

final class _ControlledDelay {
  final List<Completer<void>> _pending = <Completer<void>>[];

  int get pending => _pending.where((item) => !item.isCompleted).length;

  Future<void> call(Duration _) {
    final completer = Completer<void>();
    _pending.add(completer);
    return completer.future;
  }

  void completeNext() {
    _pending.firstWhere((item) => !item.isCompleted).complete();
  }
}
