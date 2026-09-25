// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workspace_references_controller.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(workspaceReferencesQuery)
final workspaceReferencesQueryProvider = WorkspaceReferencesQueryProvider._();

final class WorkspaceReferencesQueryProvider
    extends
        $FunctionalProvider<
          WorkspaceReferencesQueryPort,
          WorkspaceReferencesQueryPort,
          WorkspaceReferencesQueryPort
        >
    with $Provider<WorkspaceReferencesQueryPort> {
  WorkspaceReferencesQueryProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workspaceReferencesQueryProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$workspaceReferencesQueryHash();

  @$internal
  @override
  $ProviderElement<WorkspaceReferencesQueryPort> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  WorkspaceReferencesQueryPort create(Ref ref) {
    return workspaceReferencesQuery(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(WorkspaceReferencesQueryPort value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<WorkspaceReferencesQueryPort>(value),
    );
  }
}

String _$workspaceReferencesQueryHash() =>
    r'f7586b1202f7a0fb16b5b5778585bc72b0696050';

@ProviderFor(WorkspaceReferencesController)
final workspaceReferencesControllerProvider =
    WorkspaceReferencesControllerFamily._();

final class WorkspaceReferencesControllerProvider
    extends
        $NotifierProvider<
          WorkspaceReferencesController,
          WorkspaceReferencesState
        > {
  WorkspaceReferencesControllerProvider._({
    required WorkspaceReferencesControllerFamily super.from,
    required String super.argument,
  }) : super(
         retry: null,
         name: r'workspaceReferencesControllerProvider',
         isAutoDispose: false,
         dependencies: null,
         $allTransitiveDependencies: null,
       );

  @override
  String debugGetCreateSourceHash() => _$workspaceReferencesControllerHash();

  @override
  String toString() {
    return r'workspaceReferencesControllerProvider'
        ''
        '($argument)';
  }

  @$internal
  @override
  WorkspaceReferencesController create() => WorkspaceReferencesController();

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(WorkspaceReferencesState value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<WorkspaceReferencesState>(value),
    );
  }

  @override
  bool operator ==(Object other) {
    return other is WorkspaceReferencesControllerProvider &&
        other.argument == argument;
  }

  @override
  int get hashCode {
    return argument.hashCode;
  }
}

String _$workspaceReferencesControllerHash() =>
    r'25784aef941997719bdbf099e3a9462bcc8ec0f0';

final class WorkspaceReferencesControllerFamily extends $Family
    with
        $ClassFamilyOverride<
          WorkspaceReferencesController,
          WorkspaceReferencesState,
          WorkspaceReferencesState,
          WorkspaceReferencesState,
          String
        > {
  WorkspaceReferencesControllerFamily._()
    : super(
        retry: null,
        name: r'workspaceReferencesControllerProvider',
        dependencies: null,
        $allTransitiveDependencies: null,
        isAutoDispose: false,
      );

  WorkspaceReferencesControllerProvider call(String workspaceId) =>
      WorkspaceReferencesControllerProvider._(
        argument: workspaceId,
        from: this,
      );

  @override
  String toString() => r'workspaceReferencesControllerProvider';
}

abstract class _$WorkspaceReferencesController
    extends $Notifier<WorkspaceReferencesState> {
  late final _$args = ref.$arg as String;
  String get workspaceId => _$args;

  WorkspaceReferencesState build(String workspaceId);
  @$mustCallSuper
  @override
  WhenComplete runBuild() {
    final ref =
        this.ref as $Ref<WorkspaceReferencesState, WorkspaceReferencesState>;
    final element =
        ref.element
            as $ClassProviderElement<
              AnyNotifier<WorkspaceReferencesState, WorkspaceReferencesState>,
              WorkspaceReferencesState,
              Object?,
              Object?
            >;
    return element.handleCreate(ref, () => build(_$args));
  }
}
