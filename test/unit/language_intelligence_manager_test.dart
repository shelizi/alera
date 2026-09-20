import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_document_session_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_manager.dart';
import 'package:alera/src/features/language_intelligence/application/language_navigation_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/application/language_semantic_adapter_factory.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_session_manager.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_server_session_state.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late LanguageId rust;
  late LanguageExtensionRegistry registry;
  late _FakeRuntime runtime;
  late _FakeSemanticFactory semanticFactory;
  late LanguageServerSessionManager sessions;
  late LanguageIntelligenceManager manager;

  setUp(() {
    rust = LanguageId('rust');
    registry = _registry(rust);
    runtime = _FakeRuntime();
    semanticFactory = _FakeSemanticFactory();
    sessions = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
      restartBackoff: Duration.zero,
    );
    manager = LanguageIntelligenceManager(
      registry: registry,
      sessions: sessions,
      semanticAdapterFactory: semanticFactory,
    );
  });

  test(
    'disabled and unsupported documents never create a semantic adapter',
    () async {
      final disabled = await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: LanguageIntelligenceSettings.defaults,
        target: LanguageServerTarget.localWorkspace,
      );
      final unsupported = await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\README.md',
        text: '# readme',
        settings: _enabledSettings(rust),
        target: LanguageServerTarget.localWorkspace,
      );

      expect(disabled.language, rust);
      expect(disabled.session?.state, LanguageServerSessionState.disabled);
      expect(unsupported.language, isNull);
      expect(unsupported.session, isNull);
      expect(runtime.startCalls, 0);
      expect(semanticFactory.created, isEmpty);
    },
  );

  test('workspace documents share one binding and document operations stay generic', () async {
    final settings = _enabledSettings(rust);
    final first = await manager.openDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      path: r'C:\repo\src\main.rs',
      text: 'fn main() {}',
      settings: settings,
      target: LanguageServerTarget.localWorkspace,
    );
    await manager.openDocument(
      workspaceId: 'workspace-a',
      workspaceRoot: r'C:\repo',
      path: r'C:\repo\src\lib.rs',
      text: 'pub fn helper() {}',
      settings: settings,
      target: LanguageServerTarget.localWorkspace,
    );

    expect(first.semanticReady, isTrue);
    expect(runtime.startCalls, 1);
    expect(semanticFactory.created, hasLength(1));
    final binding = semanticFactory.created.single;
    expect(binding.documents.opens, hasLength(2));

    await manager.replaceDocument(
      workspaceId: 'workspace-a',
      path: r'C:\repo\src\main.rs',
      text: 'fn main() { helper(); }',
    );
    await manager.saveDocument(
      workspaceId: 'workspace-a',
      path: r'C:\repo\src\main.rs',
    );
    final target = SourceLocation(
      workspaceId: 'workspace-a',
      path: r'C:\repo\src\lib.rs',
      range: const SourceRange(
        start: SourcePosition(line: 0, scalarColumn: 7),
        end: SourcePosition(line: 0, scalarColumn: 13),
      ),
    );
    binding.navigation.definitionResult = <SourceLocation>[target];

    final locations = await manager.definition(
      workspaceId: 'workspace-a',
      path: r'C:\repo\src\main.rs',
      position: const SourcePosition(line: 0, scalarColumn: 12),
    );

    expect(binding.documents.replacements.single.text, contains('helper'));
    expect(binding.documents.saves, <String>[r'C:\repo\src\main.rs']);
    expect(locations, <SourceLocation>[target]);
    expect(binding.navigation.definitionCalls, hasLength(1));
  });

  test(
    'enabled document uses the language default provider when none is selected',
    () async {
      final state = await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: LanguageIntelligenceSettings().withLanguage(
          rust,
          const LanguageActivationSettings(enabled: true),
        ),
        target: LanguageServerTarget.localWorkspace,
      );

      expect(state.providerId, 'rust-semantic');
      expect(state.semanticReady, isTrue);
      expect(semanticFactory.created, hasLength(1));
    },
  );

  test(
    'provider generation change recreates binding and resyncs latest documents',
    () async {
      final settings = _enabledSettings(rust);
      await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\lib.rs',
        text: 'pub fn helper() {}',
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      );
      await manager.replaceDocument(
        workspaceId: 'workspace-a',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() { helper(); }',
      );
      final firstBinding = semanticFactory.created.single;

      runtime.crashLatest(exitCode: 17);
      await _flushAsync();
      await _flushAsync();
      expect(runtime.startCalls, 2);

      final locations = await manager.definition(
        workspaceId: 'workspace-a',
        path: r'C:\repo\src\main.rs',
        position: const SourcePosition(line: 0, scalarColumn: 12),
      );

      expect(locations, isEmpty);
      expect(semanticFactory.created, hasLength(2));
      final restartedBinding = semanticFactory.created.last;
      expect(restartedBinding, isNot(same(firstBinding)));
      expect(restartedBinding.documents.opens, hasLength(2));
      expect(
        restartedBinding.documents.opens
            .firstWhere((open) => open.path.endsWith('main.rs'))
            .text,
        'fn main() { helper(); }',
      );
      expect(firstBinding.navigation.definitionCalls, isEmpty);
      expect(restartedBinding.navigation.definitionCalls, hasLength(1));
    },
  );

  test(
    'close detaches the document and removes an unused semantic binding',
    () async {
      await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: _enabledSettings(rust),
        target: LanguageServerTarget.localWorkspace,
      );
      final binding = semanticFactory.created.single;

      await manager.closeDocument(
        workspaceId: 'workspace-a',
        path: r'C:\repo\src\main.rs',
      );

      expect(binding.documents.closes, <String>[r'C:\repo\src\main.rs']);
      expect(
        sessions
            .snapshotFor('workspace-a', 'rust-semantic')
            .activeDocumentCount,
        0,
      );
      expect(
        await manager.definition(
          workspaceId: 'workspace-a',
          path: r'C:\repo\src\main.rs',
          position: const SourcePosition(line: 0, scalarColumn: 0),
        ),
        isEmpty,
      );
    },
  );

  test(
    'reopening an enabled document with disabled settings detaches semantics',
    () async {
      await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: _enabledSettings(rust),
        target: LanguageServerTarget.localWorkspace,
      );
      final binding = semanticFactory.created.single;

      final state = await manager.openDocument(
        workspaceId: 'workspace-a',
        workspaceRoot: r'C:\repo',
        path: r'C:\repo\src\main.rs',
        text: 'fn main() {}',
        settings: LanguageIntelligenceSettings.defaults,
        target: LanguageServerTarget.localWorkspace,
      );

      expect(state.session?.state, LanguageServerSessionState.disabled);
      expect(binding.documents.closes, <String>[r'C:\repo\src\main.rs']);
      expect(runtime.stopCalls, 1);
      expect(
        sessions
            .snapshotFor('workspace-a', 'rust-semantic')
            .activeDocumentCount,
        0,
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

final class _FakeRuntime implements LanguageServerRuntimePort {
  int startCalls = 0;
  int stopCalls = 0;
  final List<_FakeRuntimeSession> sessions = <_FakeRuntimeSession>[];
  final Map<LanguageServerRuntimeSession, StreamController<LanguageServerExit>>
  exits =
      <LanguageServerRuntimeSession, StreamController<LanguageServerExit>>{};

  @override
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  }) async => const LanguageServerExecutableResolved('rust-analyzer');

  @override
  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  ) async {
    startCalls += 1;
    final session = _FakeRuntimeSession('session-$startCalls');
    sessions.add(session);
    exits[session] = StreamController<LanguageServerExit>.broadcast();
    return session;
  }

  @override
  Stream<LanguageServerExit> observeExit(
    LanguageServerRuntimeSession session,
  ) => exits[session]!.stream;

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {
    stopCalls += 1;
    await exits[session]?.close();
  }

  void crashLatest({required int exitCode}) {
    exits[sessions.last]!.add(LanguageServerExit(exitCode: exitCode));
  }
}

final class _FakeSemanticFactory implements LanguageSemanticAdapterFactory {
  final List<_FakeBinding> created = <_FakeBinding>[];

  @override
  LanguageSemanticProviderBinding create({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
    required LanguageServerRuntimeSession session,
  }) {
    final binding = _FakeBinding(session);
    created.add(binding);
    return LanguageSemanticProviderBinding(
      documents: binding.documents,
      navigation: binding.navigation,
    );
  }
}

final class _FakeBinding {
  _FakeBinding(this.session);

  final LanguageServerRuntimeSession session;
  final _FakeDocumentPort documents = _FakeDocumentPort();
  final _FakeNavigationPort navigation = _FakeNavigationPort();
}

final class _DocumentOpen {
  const _DocumentOpen(this.language, this.path, this.text);

  final LanguageId language;
  final String path;
  final String text;
}

final class _DocumentReplacement {
  const _DocumentReplacement(this.path, this.text);

  final String path;
  final String text;
}

final class _FakeDocumentPort implements LanguageDocumentSessionPort {
  final List<_DocumentOpen> opens = <_DocumentOpen>[];
  final List<_DocumentReplacement> replacements = <_DocumentReplacement>[];
  final List<String> saves = <String>[];
  final List<String> closes = <String>[];

  @override
  Future<void> openDocument({
    required LanguageId language,
    required String path,
    required String text,
  }) async {
    opens.add(_DocumentOpen(language, path, text));
  }

  @override
  Future<void> replaceDocument({
    required String path,
    required String text,
  }) async {
    replacements.add(_DocumentReplacement(path, text));
  }

  @override
  Future<void> saveDocument({required String path}) async {
    saves.add(path);
  }

  @override
  Future<void> closeDocument({required String path}) async {
    closes.add(path);
  }
}

final class _NavigationCall {
  const _NavigationCall(this.path, this.position);

  final String path;
  final SourcePosition position;
}

final class _FakeNavigationPort implements LanguageNavigationPort {
  List<SourceLocation> definitionResult = const <SourceLocation>[];
  final List<_NavigationCall> definitionCalls = <_NavigationCall>[];

  @override
  Future<List<SourceLocation>> definition({
    required String path,
    required SourcePosition position,
  }) async {
    definitionCalls.add(_NavigationCall(path, position));
    return definitionResult;
  }

  @override
  Future<List<SourceLocation>> declaration({
    required String path,
    required SourcePosition position,
  }) async => const <SourceLocation>[];

  @override
  Future<List<SourceLocation>> typeDefinition({
    required String path,
    required SourcePosition position,
  }) async => const <SourceLocation>[];

  @override
  Future<List<SourceLocation>> implementation({
    required String path,
    required SourcePosition position,
  }) async => const <SourceLocation>[];

  @override
  Future<List<SourceLocation>> references({
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  }) async => const <SourceLocation>[];
}
