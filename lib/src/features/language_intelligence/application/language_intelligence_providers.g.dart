// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'language_intelligence_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(languageExtensionRegistry)
final languageExtensionRegistryProvider = LanguageExtensionRegistryProvider._();

final class LanguageExtensionRegistryProvider
    extends
        $FunctionalProvider<
          LanguageExtensionRegistry,
          LanguageExtensionRegistry,
          LanguageExtensionRegistry
        >
    with $Provider<LanguageExtensionRegistry> {
  LanguageExtensionRegistryProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'languageExtensionRegistryProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$languageExtensionRegistryHash();

  @$internal
  @override
  $ProviderElement<LanguageExtensionRegistry> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  LanguageExtensionRegistry create(Ref ref) {
    return languageExtensionRegistry(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(LanguageExtensionRegistry value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<LanguageExtensionRegistry>(value),
    );
  }
}

String _$languageExtensionRegistryHash() =>
    r'4cc3243ba8ec4300fd28ebe42f0d0de8d66c7a5f';

@ProviderFor(languageServerRuntime)
final languageServerRuntimeProvider = LanguageServerRuntimeProvider._();

final class LanguageServerRuntimeProvider
    extends
        $FunctionalProvider<
          LanguageServerRuntimePort,
          LanguageServerRuntimePort,
          LanguageServerRuntimePort
        >
    with $Provider<LanguageServerRuntimePort> {
  LanguageServerRuntimeProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'languageServerRuntimeProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$languageServerRuntimeHash();

  @$internal
  @override
  $ProviderElement<LanguageServerRuntimePort> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  LanguageServerRuntimePort create(Ref ref) {
    return languageServerRuntime(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(LanguageServerRuntimePort value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<LanguageServerRuntimePort>(value),
    );
  }
}

String _$languageServerRuntimeHash() =>
    r'33e6955213d7e4638b0a0816d83b6a6b66c00634';

@ProviderFor(languageIntelligenceStatusPort)
final languageIntelligenceStatusPortProvider =
    LanguageIntelligenceStatusPortProvider._();

final class LanguageIntelligenceStatusPortProvider
    extends
        $FunctionalProvider<
          LanguageIntelligenceStatusPort,
          LanguageIntelligenceStatusPort,
          LanguageIntelligenceStatusPort
        >
    with $Provider<LanguageIntelligenceStatusPort> {
  LanguageIntelligenceStatusPortProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'languageIntelligenceStatusPortProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$languageIntelligenceStatusPortHash();

  @$internal
  @override
  $ProviderElement<LanguageIntelligenceStatusPort> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  LanguageIntelligenceStatusPort create(Ref ref) {
    return languageIntelligenceStatusPort(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(LanguageIntelligenceStatusPort value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<LanguageIntelligenceStatusPort>(
        value,
      ),
    );
  }
}

String _$languageIntelligenceStatusPortHash() =>
    r'acd43d171307dd5a067547a0003e756cf901d564';
