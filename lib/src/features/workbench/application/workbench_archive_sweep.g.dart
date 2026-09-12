// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'workbench_archive_sweep.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning
/// Session-local collapse state for Archived group headers, keyed by
/// `WorkbenchArchivedHeaderRow.key`. Deliberately kept out of
/// [WorkbenchViewPrefs] so it never persists or syncs to other clients.

@ProviderFor(WorkbenchArchivedSectionsCollapse)
final workbenchArchivedSectionsCollapseProvider =
    WorkbenchArchivedSectionsCollapseProvider._();

/// Session-local collapse state for Archived group headers, keyed by
/// `WorkbenchArchivedHeaderRow.key`. Deliberately kept out of
/// [WorkbenchViewPrefs] so it never persists or syncs to other clients.
final class WorkbenchArchivedSectionsCollapseProvider
    extends $NotifierProvider<WorkbenchArchivedSectionsCollapse, Set<String>> {
  /// Session-local collapse state for Archived group headers, keyed by
  /// `WorkbenchArchivedHeaderRow.key`. Deliberately kept out of
  /// [WorkbenchViewPrefs] so it never persists or syncs to other clients.
  WorkbenchArchivedSectionsCollapseProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workbenchArchivedSectionsCollapseProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() =>
      _$workbenchArchivedSectionsCollapseHash();

  @$internal
  @override
  WorkbenchArchivedSectionsCollapse create() =>
      WorkbenchArchivedSectionsCollapse();

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(Set<String> value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<Set<String>>(value),
    );
  }
}

String _$workbenchArchivedSectionsCollapseHash() =>
    r'd25ef49c14d2d88404eaaa80160910188899e7a4';

/// Session-local collapse state for Archived group headers, keyed by
/// `WorkbenchArchivedHeaderRow.key`. Deliberately kept out of
/// [WorkbenchViewPrefs] so it never persists or syncs to other clients.

abstract class _$WorkbenchArchivedSectionsCollapse
    extends $Notifier<Set<String>> {
  Set<String> build();
  @$mustCallSuper
  @override
  WhenComplete runBuild() {
    final ref = this.ref as $Ref<Set<String>, Set<String>>;
    final element =
        ref.element
            as $ClassProviderElement<
              AnyNotifier<Set<String>, Set<String>>,
              Set<String>,
              Object?,
              Object?
            >;
    return element.handleCreate(ref, build);
  }
}

/// Archives idle workspaces once the workbench has bootstrapped and then once
/// per [workbenchArchiveSweepInterval]. The threshold comes from
/// `GeneralSettings.autoArchiveWorkspacesAfterDays`; 0 disables the sweep.

@ProviderFor(workbenchArchiveSweepCoordinator)
final workbenchArchiveSweepCoordinatorProvider =
    WorkbenchArchiveSweepCoordinatorProvider._();

/// Archives idle workspaces once the workbench has bootstrapped and then once
/// per [workbenchArchiveSweepInterval]. The threshold comes from
/// `GeneralSettings.autoArchiveWorkspacesAfterDays`; 0 disables the sweep.

final class WorkbenchArchiveSweepCoordinatorProvider
    extends $FunctionalProvider<void, void, void>
    with $Provider<void> {
  /// Archives idle workspaces once the workbench has bootstrapped and then once
  /// per [workbenchArchiveSweepInterval]. The threshold comes from
  /// `GeneralSettings.autoArchiveWorkspacesAfterDays`; 0 disables the sweep.
  WorkbenchArchiveSweepCoordinatorProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'workbenchArchiveSweepCoordinatorProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$workbenchArchiveSweepCoordinatorHash();

  @$internal
  @override
  $ProviderElement<void> $createElement($ProviderPointer pointer) =>
      $ProviderElement(pointer);

  @override
  void create(Ref ref) {
    return workbenchArchiveSweepCoordinator(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(void value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<void>(value),
    );
  }
}

String _$workbenchArchiveSweepCoordinatorHash() =>
    r'a4fcf096d47b1eecb6db5a456a526bb0fa407182';
