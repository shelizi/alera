// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'external_editor_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(externalEditorLauncher)
final externalEditorLauncherProvider = ExternalEditorLauncherProvider._();

final class ExternalEditorLauncherProvider
    extends
        $FunctionalProvider<
          ExternalEditorLauncher,
          ExternalEditorLauncher,
          ExternalEditorLauncher
        >
    with $Provider<ExternalEditorLauncher> {
  ExternalEditorLauncherProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'externalEditorLauncherProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$externalEditorLauncherHash();

  @$internal
  @override
  $ProviderElement<ExternalEditorLauncher> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  ExternalEditorLauncher create(Ref ref) {
    return externalEditorLauncher(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(ExternalEditorLauncher value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<ExternalEditorLauncher>(value),
    );
  }
}

String _$externalEditorLauncherHash() =>
    r'7567894970dd766a68cb32322b397a069f43515c';
