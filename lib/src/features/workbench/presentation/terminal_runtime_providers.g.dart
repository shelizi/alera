// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'terminal_runtime_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(terminalRuntime)
final terminalRuntimeProvider = TerminalRuntimeProvider._();

final class TerminalRuntimeProvider
    extends
        $FunctionalProvider<TerminalRuntime, TerminalRuntime, TerminalRuntime>
    with $Provider<TerminalRuntime> {
  TerminalRuntimeProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'terminalRuntimeProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$terminalRuntimeHash();

  @$internal
  @override
  $ProviderElement<TerminalRuntime> $createElement($ProviderPointer pointer) =>
      $ProviderElement(pointer);

  @override
  TerminalRuntime create(Ref ref) {
    return terminalRuntime(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(TerminalRuntime value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<TerminalRuntime>(value),
    );
  }
}

String _$terminalRuntimeHash() => r'c5c1b5ea253e768a222c26476e7962152f2ab83f';
