import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';

WorkbenchViewPrefs focusWorkbenchSourceControlRootPrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
  required String relativeRoot,
}) {
  return prefs.copyWith(
    sourceControlRootByWorkspaceId: <String, String>{
      ...prefs.sourceControlRootByWorkspaceId,
      workspaceId: relativeRoot,
    },
    activeContextPanelTab: WorkbenchContextPanelTab.gitDiff,
    rightSidebarVisible: true,
  );
}

WorkbenchViewPrefs? clearWorkbenchSourceControlRootPrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
}) {
  if (!prefs.sourceControlRootByWorkspaceId.containsKey(workspaceId)) {
    return null;
  }
  final roots = Map<String, String>.from(prefs.sourceControlRootByWorkspaceId)
    ..remove(workspaceId);
  return prefs.copyWith(sourceControlRootByWorkspaceId: roots);
}

WorkbenchViewPrefs? syncWorkbenchSourceControlRootAfterPathMovePrefs({
  required WorkbenchViewPrefs prefs,
  required String workspaceId,
  required String oldRelativePath,
  required String newRelativePath,
}) {
  final current = prefs.sourceControlRootByWorkspaceId[workspaceId];
  if (current == null) {
    return null;
  }
  final nextRoot = _replaceSourceControlPathPrefix(
    path: current,
    oldPath: oldRelativePath,
    newPath: newRelativePath,
  );
  if (nextRoot == null || nextRoot == current) {
    return null;
  }
  return prefs.copyWith(
    sourceControlRootByWorkspaceId: <String, String>{
      ...prefs.sourceControlRootByWorkspaceId,
      workspaceId: nextRoot,
    },
  );
}

String? _replaceSourceControlPathPrefix({
  required String path,
  required String oldPath,
  required String newPath,
}) {
  final normalizedPath = normalizeSourceControlRootRelativePath(path);
  final normalizedOld = normalizeSourceControlRootRelativePath(oldPath);
  final normalizedNew = normalizeSourceControlRootRelativePath(newPath);
  if (normalizedPath == null ||
      normalizedOld == null ||
      normalizedNew == null) {
    return null;
  }
  if (normalizedPath == normalizedOld) {
    return normalizedNew;
  }
  final prefix = '$normalizedOld/';
  if (!normalizedPath.startsWith(prefix)) {
    return null;
  }
  return '$normalizedNew/${normalizedPath.substring(prefix.length)}';
}
