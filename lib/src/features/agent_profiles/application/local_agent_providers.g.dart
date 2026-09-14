// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'local_agent_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(localAgentDetection)
final localAgentDetectionProvider = LocalAgentDetectionProvider._();

final class LocalAgentDetectionProvider
    extends
        $FunctionalProvider<
          LocalAgentDetection,
          LocalAgentDetection,
          LocalAgentDetection
        >
    with $Provider<LocalAgentDetection> {
  LocalAgentDetectionProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'localAgentDetectionProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$localAgentDetectionHash();

  @$internal
  @override
  $ProviderElement<LocalAgentDetection> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  LocalAgentDetection create(Ref ref) {
    return localAgentDetection(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(LocalAgentDetection value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<LocalAgentDetection>(value),
    );
  }
}

String _$localAgentDetectionHash() =>
    r'13818982f80c0f8381c0448305884da522cbbee2';

/// Agent CLIs found on this machine's PATH. Probed once per session and kept
/// alive so the workspace tab strip's new-tab menu does not rescan on every
/// open; `ref.invalidate(installedAgentClisProvider)` forces a fresh probe.

@ProviderFor(installedAgentClis)
final installedAgentClisProvider = InstalledAgentClisProvider._();

/// Agent CLIs found on this machine's PATH. Probed once per session and kept
/// alive so the workspace tab strip's new-tab menu does not rescan on every
/// open; `ref.invalidate(installedAgentClisProvider)` forces a fresh probe.

final class InstalledAgentClisProvider
    extends
        $FunctionalProvider<
          AsyncValue<List<AgentType>>,
          List<AgentType>,
          FutureOr<List<AgentType>>
        >
    with $FutureModifier<List<AgentType>>, $FutureProvider<List<AgentType>> {
  /// Agent CLIs found on this machine's PATH. Probed once per session and kept
  /// alive so the workspace tab strip's new-tab menu does not rescan on every
  /// open; `ref.invalidate(installedAgentClisProvider)` forces a fresh probe.
  InstalledAgentClisProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'installedAgentClisProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$installedAgentClisHash();

  @$internal
  @override
  $FutureProviderElement<List<AgentType>> $createElement(
    $ProviderPointer pointer,
  ) => $FutureProviderElement(pointer);

  @override
  FutureOr<List<AgentType>> create(Ref ref) {
    return installedAgentClis(ref);
  }
}

String _$installedAgentClisHash() =>
    r'f8898b69e94d7d2c3d56889b7fb5cec97eb4cd49';
