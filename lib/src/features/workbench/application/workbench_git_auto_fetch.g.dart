// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workbench_git_auto_fetch.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning
/// Owned by the primary shell, not by Git panel visibility. Stopping the
/// provider (or disabling the setting) cancels subsequent scheduled runs.

@ProviderFor(workbenchGitAutoFetchCoordinator)
final workbenchGitAutoFetchCoordinatorProvider =
    WorkbenchGitAutoFetchCoordinatorProvider._();

/// Owned by the primary shell, not by Git panel visibility. Stopping the
/// provider (or disabling the setting) cancels subsequent scheduled runs.

final class WorkbenchGitAutoFetchCoordinatorProvider
    extends
        $FunctionalProvider<
          GitAutoFetchScheduler,
          GitAutoFetchScheduler,
          GitAutoFetchScheduler
        >
    with $Provider<GitAutoFetchScheduler> {
  /// Owned by the primary shell, not by Git panel visibility. Stopping the
  /// provider (or disabling the setting) cancels subsequent scheduled runs.
  WorkbenchGitAutoFetchCoordinatorProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workbenchGitAutoFetchCoordinatorProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$workbenchGitAutoFetchCoordinatorHash();

  @$internal
  @override
  $ProviderElement<GitAutoFetchScheduler> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  GitAutoFetchScheduler create(Ref ref) {
    return workbenchGitAutoFetchCoordinator(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(GitAutoFetchScheduler value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<GitAutoFetchScheduler>(value),
    );
  }
}

String _$workbenchGitAutoFetchCoordinatorHash() =>
    r'0b5fb02eb1f638e8d4593181d30ce5be3c69989a';
