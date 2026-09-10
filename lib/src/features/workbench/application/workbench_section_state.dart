import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

WorkbenchState applyWorkbenchSectionSnapshotState({
  required WorkbenchState state,
  required WorkspaceSectionSnapshot snapshot,
}) {
  final sections = snapshot.sections;
  final ids = sections.map((section) => section.id).toSet();
  final prefs = state.viewPrefs;
  return state.copyWith(
    supportsSections: snapshot.supported,
    sections: sections,
    viewPrefs: prefs.copyWith(
      groupBy: !snapshot.supported && prefs.groupBy == WorkbenchGroupBy.section
          ? WorkbenchGroupBy.project
          : prefs.groupBy,
      collapsedSectionIds: snapshot.supported
          ? prefs.collapsedSectionIds.intersection(ids)
          : prefs.collapsedSectionIds,
    ),
  );
}

WorkbenchState toggleWorkbenchSectionCollapsedState({
  required WorkbenchState state,
  required String? sectionId,
}) {
  final prefs = state.viewPrefs;
  if (sectionId == null) {
    return state.copyWith(
      viewPrefs: prefs.copyWith(
        othersSectionCollapsed: !prefs.othersSectionCollapsed,
      ),
    );
  }
  final ids = Set<String>.from(prefs.collapsedSectionIds);
  if (!ids.add(sectionId)) {
    ids.remove(sectionId);
  }
  return state.copyWith(viewPrefs: prefs.copyWith(collapsedSectionIds: ids));
}
