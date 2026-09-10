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
  test('named section collapse toggle changes only collapsed section ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedSectionIds: const <String>{'a'},
      othersSectionCollapsed: true,
      selectedProjectIds: const <String>{'project'},
    );
    final state = WorkbenchState(viewPrefs: prefs, searchQuery: 'keep-search');

    final removed = toggleWorkbenchSectionCollapsedState(
      state: state,
      sectionId: 'a',
    );
    expect(removed.viewPrefs.collapsedSectionIds, isEmpty);
    expect(removed.viewPrefs.othersSectionCollapsed, isTrue);
    expect(
      removed.viewPrefs.selectedProjectIds,
      same(prefs.selectedProjectIds),
    );
    expect(removed.searchQuery, 'keep-search');

    final added = toggleWorkbenchSectionCollapsedState(
      state: state,
      sectionId: 'b',
    );
    expect(added.viewPrefs.collapsedSectionIds, <String>{'a', 'b'});
    expect(added.viewPrefs.othersSectionCollapsed, isTrue);
  });

  test('Others collapse toggle changes only the Others flag', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedSectionIds: const <String>{'a'},
      othersSectionCollapsed: false,
    );
    final state = WorkbenchState(viewPrefs: prefs);

    final next = toggleWorkbenchSectionCollapsedState(
      state: state,
      sectionId: null,
    );

    expect(next.viewPrefs.othersSectionCollapsed, isTrue);
    expect(next.viewPrefs.collapsedSectionIds, same(prefs.collapsedSectionIds));
  });
}
