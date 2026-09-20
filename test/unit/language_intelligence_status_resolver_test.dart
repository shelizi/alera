import 'package:alera/src/features/language_intelligence/application/language_intelligence_status_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_status.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late LanguageProviderDescriptor provider;
  late _FakeRuntime runtime;
  late RuntimeLanguageIntelligenceStatusPort resolver;

  setUp(() {
    provider = LanguageProviderDescriptor(
      id: 'rust.rust-analyzer',
      kind: LanguageProviderKind.semanticServer,
      languages: <LanguageId>{LanguageId('rust')},
      capabilities: const <LanguageCapability>{LanguageCapability.definition},
      processScope: LanguageProviderProcessScope.workspace,
      launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
      executableResolutionPolicy:
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
      executableCandidates: const <String>['rust-analyzer'],
    );
    runtime = _FakeRuntime();
    resolver = RuntimeLanguageIntelligenceStatusPort(runtime: runtime);
  });

  test('disabled activation never probes the executable', () async {
    final status = await resolver.resolve(
      provider: provider,
      settings: LanguageActivationSettings.defaults,
    );

    expect(status.kind, LanguageIntelligenceStatusKind.disabled);
    expect(runtime.resolveCalls, 0);
  });

  test('resolved executable reports ready without starting a server', () async {
    runtime.resolution = const LanguageServerExecutableResolved(
      'rust-analyzer',
    );

    final status = await resolver.resolve(
      provider: provider,
      settings: const LanguageActivationSettings(enabled: true),
    );

    expect(status.kind, LanguageIntelligenceStatusKind.ready);
    expect(status.executable, 'rust-analyzer');
    expect(runtime.resolveCalls, 1);
    expect(runtime.startCalls, 0);
  });

  test('missing executable preserves setup guidance', () async {
    runtime.resolution = const LanguageServerExecutableMissing(
      reason: 'rust-analyzer was not found on PATH',
    );

    final status = await resolver.resolve(
      provider: provider,
      settings: const LanguageActivationSettings(enabled: true),
    );

    expect(status.kind, LanguageIntelligenceStatusKind.missing);
    expect(status.detail, contains('PATH'));
  });

  test('resolution errors become failed status instead of escaping', () async {
    runtime.resolveError = StateError('probe failed');

    final status = await resolver.resolve(
      provider: provider,
      settings: const LanguageActivationSettings(enabled: true),
    );

    expect(status.kind, LanguageIntelligenceStatusKind.failed);
    expect(status.detail, contains('probe failed'));
  });
}

final class _FakeRuntime implements LanguageServerRuntimePort {
  LanguageServerExecutableResolution resolution =
      const LanguageServerExecutableResolved('server');
  Object? resolveError;
  int resolveCalls = 0;
  int startCalls = 0;

  @override
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  }) async {
    resolveCalls += 1;
    final error = resolveError;
    if (error != null) throw error;
    return resolution;
  }

  @override
  Stream<LanguageServerExit> observeExit(
    LanguageServerRuntimeSession session,
  ) => const Stream<LanguageServerExit>.empty();

  @override
  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  ) {
    startCalls += 1;
    throw UnimplementedError();
  }

  @override
  Future<void> stop(LanguageServerRuntimeSession session) async {}
}
