// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workspace_git_branch_compare_cache_provider.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning

@ProviderFor(workspaceGitBranchCompareCache)
final workspaceGitBranchCompareCacheProvider =
    WorkspaceGitBranchCompareCacheFamily._();

final class WorkspaceGitBranchCompareCacheProvider
    extends
        $FunctionalProvider<
          WorkspaceGitBranchCompareCache,
          WorkspaceGitBranchCompareCache,
          WorkspaceGitBranchCompareCache
        >
    with $Provider<WorkspaceGitBranchCompareCache> {
  WorkspaceGitBranchCompareCacheProvider._({
    required WorkspaceGitBranchCompareCacheFamily super.from,
    required (String, String) super.argument,
  }) : super(
         retry: null,
         name: r'workspaceGitBranchCompareCacheProvider',
         isAutoDispose: false,
         dependencies: null,
         $allTransitiveDependencies: null,
       );

  @override
  String debugGetCreateSourceHash() => _$workspaceGitBranchCompareCacheHash();

  @override
  String toString() {
    return r'workspaceGitBranchCompareCacheProvider'
        ''
        '$argument';
  }

  @$internal
  @override
  $ProviderElement<WorkspaceGitBranchCompareCache> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  WorkspaceGitBranchCompareCache create(Ref ref) {
    final argument = this.argument as (String, String);
    return workspaceGitBranchCompareCache(ref, argument.$1, argument.$2);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(WorkspaceGitBranchCompareCache value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<WorkspaceGitBranchCompareCache>(
        value,
      ),
    );
  }

  @override
  bool operator ==(Object other) {
    return other is WorkspaceGitBranchCompareCacheProvider &&
        other.argument == argument;
  }

  @override
  int get hashCode {
    return argument.hashCode;
  }
}

String _$workspaceGitBranchCompareCacheHash() =>
    r'53898ccc7c8403463e9707bfd4febdb17a4c274d';

final class WorkspaceGitBranchCompareCacheFamily extends $Family
    with
        $FunctionalFamilyOverride<
          WorkspaceGitBranchCompareCache,
          (String, String)
        > {
  WorkspaceGitBranchCompareCacheFamily._()
    : super(
        retry: null,
        name: r'workspaceGitBranchCompareCacheProvider',
        dependencies: null,
        $allTransitiveDependencies: null,
        isAutoDispose: false,
      );

  WorkspaceGitBranchCompareCacheProvider call(
    String workspaceId,
    String tabId,
  ) => WorkspaceGitBranchCompareCacheProvider._(
    argument: (workspaceId, tabId),
    from: this,
  );

  @override
  String toString() => r'workspaceGitBranchCompareCacheProvider';
}
