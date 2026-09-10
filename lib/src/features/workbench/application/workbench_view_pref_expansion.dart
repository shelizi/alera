import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

WorkbenchViewPrefs toggleWorkbenchProjectCollapsedPrefs({
  required WorkbenchViewPrefs prefs,
  required String projectId,
}) {
  return prefs.copyWith(
    collapsedProjectIds: _toggledMembership(
      prefs.collapsedProjectIds,
      projectId,
    ),
  );
}

WorkbenchViewPrefs toggleWorkbenchParentWorkspaceCollapsedPrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
}) {
  return prefs.copyWith(
    collapsedParentWorkspaceIds: _toggledMembership(
      prefs.collapsedParentWorkspaceIds,
      workspaceId,
    ),
  );
}

WorkbenchViewPrefs toggleWorkbenchWorkspaceExpandedPrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
}) {
  return prefs.copyWith(
    expandedWorkspaceIds: _toggledMembership(
      prefs.expandedWorkspaceIds,
      workspaceId,
    ),
  );
}

WorkbenchViewPrefs? nextWorkbenchWorkspaceExpandedPrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
  required bool expanded,
}) {
  final current = prefs.expandedWorkspaceIds;
  if (current.contains(workspaceId) == expanded) {
    return null;
  }
  final next = Set<String>.from(current);
  if (expanded) {
    next.add(workspaceId);
  } else {
    next.remove(workspaceId);
  }
  return prefs.copyWith(expandedWorkspaceIds: next);
}

Set<String> _toggledMembership(Set<String> current, String id) {
  final next = Set<String>.from(current);
  if (!next.add(id)) {
    next.remove(id);
  }
  return next;
}
