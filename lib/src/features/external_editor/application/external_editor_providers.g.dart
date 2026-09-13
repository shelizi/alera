// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'external_editor_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning
/// Launcher for the editor selected in Settings > Editor. Implicit flows
/// (code-open target, auto-open, keyboard shortcut fallback resolution) go
/// through this; explicit menu picks use [externalEditorLauncherForProvider].

@ProviderFor(externalEditorLauncher)
final externalEditorLauncherProvider = ExternalEditorLauncherProvider._();

/// Launcher for the editor selected in Settings > Editor. Implicit flows
/// (code-open target, auto-open, keyboard shortcut fallback resolution) go
/// through this; explicit menu picks use [externalEditorLauncherForProvider].

final class ExternalEditorLauncherProvider
    extends
        $FunctionalProvider<
          ExternalEditorLauncher,
          ExternalEditorLauncher,
          ExternalEditorLauncher
        >
    with $Provider<ExternalEditorLauncher> {
  /// Launcher for the editor selected in Settings > Editor. Implicit flows
  /// (code-open target, auto-open, keyboard shortcut fallback resolution) go
  /// through this; explicit menu picks use [externalEditorLauncherForProvider].
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
    r'96ae15b203b1768f863a14978e2472365de47641';

@ProviderFor(externalEditorLauncherFor)
final externalEditorLauncherForProvider = ExternalEditorLauncherForFamily._();

final class ExternalEditorLauncherForProvider
    extends
        $FunctionalProvider<
          ExternalEditorLauncher,
          ExternalEditorLauncher,
          ExternalEditorLauncher
        >
    with $Provider<ExternalEditorLauncher> {
  ExternalEditorLauncherForProvider._({
    required ExternalEditorLauncherForFamily super.from,
    required ExternalEditorKind super.argument,
  }) : super(
         retry: null,
         name: r'externalEditorLauncherForProvider',
         isAutoDispose: false,
         dependencies: null,
         $allTransitiveDependencies: null,
       );

  @override
  String debugGetCreateSourceHash() => _$externalEditorLauncherForHash();

  @override
  String toString() {
    return r'externalEditorLauncherForProvider'
        ''
        '($argument)';
  }

  @$internal
  @override
  $ProviderElement<ExternalEditorLauncher> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  ExternalEditorLauncher create(Ref ref) {
    final argument = this.argument as ExternalEditorKind;
    return externalEditorLauncherFor(ref, argument);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(ExternalEditorLauncher value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<ExternalEditorLauncher>(value),
    );
  }

  @override
  bool operator ==(Object other) {
    return other is ExternalEditorLauncherForProvider &&
        other.argument == argument;
  }

  @override
  int get hashCode {
    return argument.hashCode;
  }
}

String _$externalEditorLauncherForHash() =>
    r'05c137beb3feb42cdebf064bff8f2276fb135082';

final class ExternalEditorLauncherForFamily extends $Family
    with $FunctionalFamilyOverride<ExternalEditorLauncher, ExternalEditorKind> {
  ExternalEditorLauncherForFamily._()
    : super(
        retry: null,
        name: r'externalEditorLauncherForProvider',
        dependencies: null,
        $allTransitiveDependencies: null,
        isAutoDispose: false,
      );

  ExternalEditorLauncherForProvider call(ExternalEditorKind kind) =>
      ExternalEditorLauncherForProvider._(argument: kind, from: this);

  @override
  String toString() => r'externalEditorLauncherForProvider';
}

/// Specs whose CLI resolves on this machine right now. Resolution-only; no
/// editor process is spawned, so menus can gate on it cheaply.

@ProviderFor(installedExternalEditors)
final installedExternalEditorsProvider = InstalledExternalEditorsProvider._();

/// Specs whose CLI resolves on this machine right now. Resolution-only; no
/// editor process is spawned, so menus can gate on it cheaply.

final class InstalledExternalEditorsProvider
    extends
        $FunctionalProvider<
          AsyncValue<List<ExternalEditorSpec>>,
          List<ExternalEditorSpec>,
          FutureOr<List<ExternalEditorSpec>>
        >
    with
        $FutureModifier<List<ExternalEditorSpec>>,
        $FutureProvider<List<ExternalEditorSpec>> {
  /// Specs whose CLI resolves on this machine right now. Resolution-only; no
  /// editor process is spawned, so menus can gate on it cheaply.
  InstalledExternalEditorsProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'installedExternalEditorsProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$installedExternalEditorsHash();

  @$internal
  @override
  $FutureProviderElement<List<ExternalEditorSpec>> $createElement(
    $ProviderPointer pointer,
  ) => $FutureProviderElement(pointer);

  @override
  FutureOr<List<ExternalEditorSpec>> create(Ref ref) {
    return installedExternalEditors(ref);
  }
}

String _$installedExternalEditorsHash() =>
    r'0f27911c5ca6067b7625b8e962ef3db9ce3889a6';

/// The editor menus and shortcuts should act on: the configured kind when it
/// resolves, otherwise the first installed spec, otherwise null when nothing
/// is installed and every "Open in Editor" entry should hide.

@ProviderFor(resolvedExternalEditor)
final resolvedExternalEditorProvider = ResolvedExternalEditorProvider._();

/// The editor menus and shortcuts should act on: the configured kind when it
/// resolves, otherwise the first installed spec, otherwise null when nothing
/// is installed and every "Open in Editor" entry should hide.

final class ResolvedExternalEditorProvider
    extends
        $FunctionalProvider<
          AsyncValue<ExternalEditorSpec?>,
          ExternalEditorSpec?,
          FutureOr<ExternalEditorSpec?>
        >
    with
        $FutureModifier<ExternalEditorSpec?>,
        $FutureProvider<ExternalEditorSpec?> {
  /// The editor menus and shortcuts should act on: the configured kind when it
  /// resolves, otherwise the first installed spec, otherwise null when nothing
  /// is installed and every "Open in Editor" entry should hide.
  ResolvedExternalEditorProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'resolvedExternalEditorProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$resolvedExternalEditorHash();

  @$internal
  @override
  $FutureProviderElement<ExternalEditorSpec?> $createElement(
    $ProviderPointer pointer,
  ) => $FutureProviderElement(pointer);

  @override
  FutureOr<ExternalEditorSpec?> create(Ref ref) {
    return resolvedExternalEditor(ref);
  }
}

String _$resolvedExternalEditorHash() =>
    r'4120ceda322fb65a896732c9bc48c5c6ac1a48ac';
