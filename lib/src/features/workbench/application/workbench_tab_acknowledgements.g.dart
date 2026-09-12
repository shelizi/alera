// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workbench_tab_acknowledgements.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning
/// Session-local acknowledgement of the exact completion epoch a user viewed.
///
/// Kept as a provider rather than widget state so the sidebar row builder can
/// bucket unacked completions next to the tab strip that marks them read.
/// Deliberately not persisted: a done status restored on the next launch reads
/// as unread again.

@ProviderFor(WorkbenchTabCompletionAcknowledgementsController)
final workbenchTabCompletionAcknowledgementsControllerProvider =
    WorkbenchTabCompletionAcknowledgementsControllerProvider._();

/// Session-local acknowledgement of the exact completion epoch a user viewed.
///
/// Kept as a provider rather than widget state so the sidebar row builder can
/// bucket unacked completions next to the tab strip that marks them read.
/// Deliberately not persisted: a done status restored on the next launch reads
/// as unread again.
final class WorkbenchTabCompletionAcknowledgementsControllerProvider
    extends
        $NotifierProvider<
          WorkbenchTabCompletionAcknowledgementsController,
          Map<String, DateTime>
        > {
  /// Session-local acknowledgement of the exact completion epoch a user viewed.
  ///
  /// Kept as a provider rather than widget state so the sidebar row builder can
  /// bucket unacked completions next to the tab strip that marks them read.
  /// Deliberately not persisted: a done status restored on the next launch reads
  /// as unread again.
  WorkbenchTabCompletionAcknowledgementsControllerProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workbenchTabCompletionAcknowledgementsControllerProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() =>
      _$workbenchTabCompletionAcknowledgementsControllerHash();

  @$internal
  @override
  WorkbenchTabCompletionAcknowledgementsController create() =>
      WorkbenchTabCompletionAcknowledgementsController();

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(Map<String, DateTime> value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<Map<String, DateTime>>(value),
    );
  }
}

String _$workbenchTabCompletionAcknowledgementsControllerHash() =>
    r'85e445d57b4e31d5e7860cefdc50c857a1607b4d';

/// Session-local acknowledgement of the exact completion epoch a user viewed.
///
/// Kept as a provider rather than widget state so the sidebar row builder can
/// bucket unacked completions next to the tab strip that marks them read.
/// Deliberately not persisted: a done status restored on the next launch reads
/// as unread again.

abstract class _$WorkbenchTabCompletionAcknowledgementsController
    extends $Notifier<Map<String, DateTime>> {
  Map<String, DateTime> build();
  @$mustCallSuper
  @override
  WhenComplete runBuild() {
    final ref = this.ref as $Ref<Map<String, DateTime>, Map<String, DateTime>>;
    final element =
        ref.element
            as $ClassProviderElement<
              AnyNotifier<Map<String, DateTime>, Map<String, DateTime>>,
              Map<String, DateTime>,
              Object?,
              Object?
            >;
    return element.handleCreate(ref, build);
  }
}
