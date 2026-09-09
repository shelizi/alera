// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'external_terminal_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(externalTerminalLauncher)
final externalTerminalLauncherProvider = ExternalTerminalLauncherProvider._();

final class ExternalTerminalLauncherProvider
    extends
        $FunctionalProvider<
          ExternalTerminalLauncher,
          ExternalTerminalLauncher,
          ExternalTerminalLauncher
        >
    with $Provider<ExternalTerminalLauncher> {
  ExternalTerminalLauncherProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'externalTerminalLauncherProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$externalTerminalLauncherHash();

  @$internal
  @override
  $ProviderElement<ExternalTerminalLauncher> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  ExternalTerminalLauncher create(Ref ref) {
    return externalTerminalLauncher(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(ExternalTerminalLauncher value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<ExternalTerminalLauncher>(value),
    );
  }
}

String _$externalTerminalLauncherHash() =>
    r'94924b17f00f2363f053958ef04345c1f92771b4';
