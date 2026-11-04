// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'runtime_settings_changes.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(runtimeSettingsChanges)
final runtimeSettingsChangesProvider = RuntimeSettingsChangesProvider._();

final class RuntimeSettingsChangesProvider
    extends $FunctionalProvider<AsyncValue<int>, int, Stream<int>>
    with $FutureModifier<int>, $StreamProvider<int> {
  RuntimeSettingsChangesProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'runtimeSettingsChangesProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$runtimeSettingsChangesHash();

  @$internal
  @override
  $StreamProviderElement<int> $createElement($ProviderPointer pointer) =>
      $StreamProviderElement(pointer);

  @override
  Stream<int> create(Ref ref) {
    return runtimeSettingsChanges(ref);
  }
}

String _$runtimeSettingsChangesHash() =>
    r'd53587e057e3ef1473c2aff822ed90ce81cffa3e';
