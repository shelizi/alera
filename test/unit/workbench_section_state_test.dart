import 'package:alera/src/features/workbench/application/workbench_section_state.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace_section.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('supported section snapshot prunes stale collapsed section ids', () {
    final now = DateTime.utc(2026, 9, 10);
    final live = WorkspaceSection(
      id: 'live',
      name: 'Live',
      createdAt: now,
      updatedAt: now,
    );
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      groupBy: WorkbenchGroupBy.section,
      collapsedSectionIds: const <String>{'live', 'stale'},
      selectedProjectIds: const <String>{'project'},
    );
    final state = WorkbenchState(
      viewPrefs: prefs,
      searchQuery: 'keep-search',
      error: 'keep-error',
    );

    final next = applyWorkbenchSectionSnapshotState(
      state: state,
      snapshot: WorkspaceSectionSnapshot(
        supported: true,
        sections: <WorkspaceSection>[live],
      ),
    );

    expect(next.supportsSections, isTrue);
    expect(next.sections, <WorkspaceSection>[live]);
    expect(next.viewPrefs.groupBy, WorkbenchGroupBy.section);
    expect(next.viewPrefs.collapsedSectionIds, <String>{'live'});
    expect(next.viewPrefs.selectedProjectIds, same(prefs.selectedProjectIds));
    expect(next.searchQuery, 'keep-search');
    expect(next.error, 'keep-error');
  });

  test('unsupported section snapshot falls back grouping but preserves collapsed ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      groupBy: WorkbenchGroupBy.section,
      collapsedSectionIds: const <String>{'remember-me'},
    );
    final state = WorkbenchState(viewPrefs: prefs);

    final next = applyWorkbenchSectionSnapshotState(
      state: state,
      snapshot: const WorkspaceSectionSnapshot(supported: false),
    );

    expect(next.supportsSections, isFalse);
    expect(next.sections, isEmpty);
    expect(next.viewPrefs.groupBy, WorkbenchGroupBy.project);
    expect(next.viewPrefs.collapsedSectionIds, <String>{'remember-me'});
  });

  test('unsupported snapshot preserves a non-section grouping', () {
    final state = WorkbenchState(
      viewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        groupBy: WorkbenchGroupBy.none,
        collapsedSectionIds: const <String>{'remember-me'},
      ),
    );

    final next = applyWorkbenchSectionSnapshotState(
      state: state,
      snapshot: const WorkspaceSectionSnapshot(supported: false),
    );

    expect(next.viewPrefs.groupBy, WorkbenchGroupBy.none);
    expect(next.viewPrefs.collapsedSectionIds, <String>{'remember-me'});
  });
}
