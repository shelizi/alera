// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workspace_file_open_coordinator_provider.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(workspaceFileOpenCoordinator)
final workspaceFileOpenCoordinatorProvider =
    WorkspaceFileOpenCoordinatorProvider._();

final class WorkspaceFileOpenCoordinatorProvider
    extends
        $FunctionalProvider<
          WorkspaceFileOpenCoordinator,
          WorkspaceFileOpenCoordinator,
          WorkspaceFileOpenCoordinator
        >
    with $Provider<WorkspaceFileOpenCoordinator> {
  WorkspaceFileOpenCoordinatorProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workspaceFileOpenCoordinatorProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$workspaceFileOpenCoordinatorHash();

  @$internal
  @override
  $ProviderElement<WorkspaceFileOpenCoordinator> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  WorkspaceFileOpenCoordinator create(Ref ref) {
    return workspaceFileOpenCoordinator(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(WorkspaceFileOpenCoordinator value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<WorkspaceFileOpenCoordinator>(value),
    );
  }
}

String _$workspaceFileOpenCoordinatorHash() =>
    r'6bf4cb61a1627ccabc049e630f387539696cbcd1';
