// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'ai_assist_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(agentTaskRunner)
final agentTaskRunnerProvider = AgentTaskRunnerProvider._();

final class AgentTaskRunnerProvider
    extends
        $FunctionalProvider<AgentTaskRunner, AgentTaskRunner, AgentTaskRunner>
    with $Provider<AgentTaskRunner> {
  AgentTaskRunnerProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'agentTaskRunnerProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$agentTaskRunnerHash();

  @$internal
  @override
  $ProviderElement<AgentTaskRunner> $createElement($ProviderPointer pointer) =>
      $ProviderElement(pointer);

  @override
  AgentTaskRunner create(Ref ref) {
    return agentTaskRunner(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(AgentTaskRunner value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<AgentTaskRunner>(value),
    );
  }
}

String _$agentTaskRunnerHash() => r'b08c994e45267a719ac4e635a7113c0d2997f8e0';

@ProviderFor(aiAssistService)
final aiAssistServiceProvider = AiAssistServiceProvider._();

final class AiAssistServiceProvider
    extends
        $FunctionalProvider<AiAssistService, AiAssistService, AiAssistService>
    with $Provider<AiAssistService> {
  AiAssistServiceProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'aiAssistServiceProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$aiAssistServiceHash();

  @$internal
  @override
  $ProviderElement<AiAssistService> $createElement($ProviderPointer pointer) =>
      $ProviderElement(pointer);

  @override
  AiAssistService create(Ref ref) {
    return aiAssistService(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(AiAssistService value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<AiAssistService>(value),
    );
  }
}

String _$aiAssistServiceHash() => r'cbd7e5f1e6d3244cec9f5ff0470f78a688640d79';

@ProviderFor(aiAssistModelDiscoveryService)
final aiAssistModelDiscoveryServiceProvider =
    AiAssistModelDiscoveryServiceProvider._();

final class AiAssistModelDiscoveryServiceProvider
    extends
        $FunctionalProvider<
          AiAssistModelDiscoveryService,
          AiAssistModelDiscoveryService,
          AiAssistModelDiscoveryService
        >
    with $Provider<AiAssistModelDiscoveryService> {
  AiAssistModelDiscoveryServiceProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'aiAssistModelDiscoveryServiceProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$aiAssistModelDiscoveryServiceHash();

  @$internal
  @override
  $ProviderElement<AiAssistModelDiscoveryService> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  AiAssistModelDiscoveryService create(Ref ref) {
    return aiAssistModelDiscoveryService(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(AiAssistModelDiscoveryService value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<AiAssistModelDiscoveryService>(
        value,
      ),
    );
  }
}

String _$aiAssistModelDiscoveryServiceHash() =>
    r'12d1873ea7b3a44935fd60c9808aa501744e27fc';
