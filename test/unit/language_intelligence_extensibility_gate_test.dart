import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_document_session_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_manager.dart';
import 'package:alera/src/features/language_intelligence/application/language_navigation_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/application/language_semantic_adapter_factory.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_session_manager.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_contribution.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a synthetic language works through contribution, session, and navigation without core changes', () async {
    final moon = LanguageId('moon');
    final registry = LanguageExtensionRegistry()
      ..registerContribution(
        LanguageExtensionContribution(
          languages: <LanguageExtensionDescriptor>[
            LanguageExtensionDescriptor(
              id: moon,
              displayName: 'Moon',
              fileExtensions: const <String>['moon'],
              aliases: const <String>['mn'],
              semanticProviderIds: const <String>['moon.fake-ls'],
              defaultSemanticProviderId: 'moon.fake-ls',
              capabilities: const <LanguageCapability>{
                LanguageCapability.definition,
                LanguageCapability.references,
              },
            ),
          ],
          providers: <LanguageProviderDescriptor>[
            LanguageProviderDescriptor(
              id: 'moon.fake-ls',
              kind: LanguageProviderKind.semanticServer,
              languages: <LanguageId>{moon},
              capabilities: const <LanguageCapability>{
                LanguageCapability.definition,
                LanguageCapability.references,
              },
              processScope: LanguageProviderProcessScope.workspace,
              launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
              executableResolutionPolicy:
                  LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
              executableCandidates: const <String>['moon-ls'],
            ),
          ],
        ),
      );
    final runtime = _GateRuntime();
    final factory = _GateSemanticFactory();
    final sessions = LanguageServerSessionManager(
      registry: registry,
      runtime: runtime,
      idleShutdownDelay: Duration.zero,
      restartBackoff: Duration.zero,
    );
    final manager = LanguageIntelligenceManager(
      registry: registry,
      sessions: sessions,
      semanticAdapterFactory: factory,
    );
    final settings = LanguageIntelligenceSettings().withLanguage(
      moon,
      const LanguageActivationSettings(enabled: true),
    );
    const sourcePath = r'C:\repo\src\main.moon';

    expect(registry.languageForId('MN')?.id, moon);
    expect(registry.languageForPath(sourcePath)?.id, moon);

    final state = await manager.openDocument(
      workspaceId: 'workspace-moon',
      workspaceRoot: r'C:\repo',
      path: sourcePath,
      text: 'use crater',
      settings: settings,
      target: LanguageServerTarget.localWorkspace,
    );

    expect(state.language, moon);
    expect(state.providerId, 'moon.fake-ls');
    expect(state.semanticReady, isTrue);
    expect(runtime.startRequests, hasLength(1));
    expect(runtime.startRequests.single.provider.id, 'moon.fake-ls');
    expect(runtime.startRequests.single.executable, 'moon-ls');
    expect(factory.providers, <String>['moon.fake-ls']);
    expect(factory.documents.opens.single.language, moon);

    const definition = SourceLocation(
      workspaceId: 'workspace-moon',
      path: r'C:\repo\src\crater.moon',
      range: SourceRange(
        start: SourcePosition(line: 3, scalarColumn: 2),
        end: SourcePosition(line: 3, scalarColumn: 8),
      ),
    );
    const reference = SourceLocation(
      workspaceId: 'workspace-moon',
      path: sourcePath,
      range: SourceRange(
        start: SourcePosition(line: 0, scalarColumn: 4),
        end: SourcePosition(line: 0, scalarColumn: 10),
      ),
    );
    factory.navigation
      ..definitionResult = const <SourceLocation>[definition]
      ..referencesResult = const <SourceLocation>[definition, reference];

    final definitions = await manager.definition(
      workspaceId: 'workspace-moon',
      path: sourcePath,
      position: const SourcePosition(line: 0, scalarColumn: 5),
    );
    final references = await manager.references(
      workspaceId: 'workspace-moon',
      path: sourcePath,
      position: const SourcePosition(line: 0, scalarColumn: 5),
    );

    expect(definitions, const <SourceLocation>[definition]);
    expect(references, const <SourceLocation>[definition, reference]);
    expect(factory.navigation.definitionCalls, 1);
    expect(factory.navigation.referencesCalls, 1);

    await manager.closeDocument(
      workspaceId: 'workspace-moon',
      path: sourcePath,
    );
    await Future<void>.delayed(Duration.zero);

    expect(factory.documents.closes, <String>[sourcePath]);
    expect(
      sessions
          .snapshotFor('workspace-moon', 'moon.fake-ls')
          .activeDocumentCount,
      0,
    );
  });
}

final class _GateRuntimeSession implements LanguageServerRuntimeSession {
  const _GateRuntimeSession();
}

final class _GateRuntime implements LanguageServerRuntimePort {
  final List<LanguageServerRuntimeStartRequest> startRequests =
      <LanguageServerRuntimeStartRequest>[];
  final StreamController<LanguageServerExit> exits =
      StreamController<LanguageServerExit>.broadcast();

  @override
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  }) async => const LanguageServerExecutableResolved('moon-ls');

  @override
  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  ) async {
    startRequests.add(request);
    return const _GateRuntimeSession();
  }

  @override
  Stream<LanguageServerExit> observeExit(
    LanguageServerRuntimeSession session,
  ) => exits.stream;

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {}
}

final class _GateSemanticFactory implements LanguageSemanticAdapterFactory {
  final _GateDocuments documents = _GateDocuments();
  final _GateNavigation navigation = _GateNavigation();
  final List<String> providers = <String>[];

  @override
  LanguageSemanticProviderBinding create({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
    required LanguageServerRuntimeSession session,
  }) {
    providers.add(provider.id);
    return LanguageSemanticProviderBinding(
      documents: documents,
      navigation: navigation,
    );
  }
}

final class _GateOpen {
  const _GateOpen(this.language, this.path, this.text);

  final LanguageId language;
  final String path;
  final String text;
}

final class _GateDocuments implements LanguageDocumentSessionPort {
  final List<_GateOpen> opens = <_GateOpen>[];
  final List<String> closes = <String>[];

  @override
  Future<void> openDocument({
    required LanguageId language,
    required String path,
    required String text,
  }) async {
    opens.add(_GateOpen(language, path, text));
  }

  @override
  Future<void> replaceDocument({
    required String path,
    required String text,
  }) async {}

  @override
  Future<void> saveDocument({required String path}) async {}

  @override
  Future<void> closeDocument({required String path}) async {
    closes.add(path);
  }
}

final class _GateNavigation implements LanguageNavigationPort {
  List<SourceLocation> definitionResult = const <SourceLocation>[];
  List<SourceLocation> referencesResult = const <SourceLocation>[];
  int definitionCalls = 0;
  int referencesCalls = 0;

  @override
  Future<List<SourceLocation>> definition({
    required String path,
    required SourcePosition position,
  }) async {
    definitionCalls += 1;
    return definitionResult;
  }

  @override
  Future<List<SourceLocation>> references({
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  }) async {
    referencesCalls += 1;
    return referencesResult;
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
}
