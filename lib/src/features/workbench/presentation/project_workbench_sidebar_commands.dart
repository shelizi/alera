part of 'project_workbench_sidebar.dart';

final class _WorkbenchSidebarCommands {
  const _WorkbenchSidebarCommands({
    required this.toggleSectionCollapsed,
    required this.deleteWorkspaceSection,
    required this.togglePinnedSectionCollapsed,
    required this.toggleAllSectionCollapsed,
    required this.toggleProjectCollapsed,
    required this.reconcileProjectWorkspaces,
    required this.toggleParentWorkspaceCollapsed,
    required this.toggleWorkspaceExpanded,
    required this.showWorkspaceSection,
    required this.clearWorkspaceSection,
    required this.setCollapsed,
    required this.activateProject,
  });

  final void Function(String? sectionId) toggleSectionCollapsed;
  final Future<void> Function(String sectionId) deleteWorkspaceSection;
  final VoidCallback togglePinnedSectionCollapsed;
  final VoidCallback toggleAllSectionCollapsed;
  final void Function(String projectId) toggleProjectCollapsed;
  final Future<void> Function(String projectId) reconcileProjectWorkspaces;
  final void Function(String workspaceId) toggleParentWorkspaceCollapsed;
  final void Function(String workspaceId) toggleWorkspaceExpanded;
  final Future<void> Function(Workspace workspace) showWorkspaceSection;
  final Future<void> Function(Workspace workspace) clearWorkspaceSection;
  final void Function(bool collapsed) setCollapsed;
  final Future<void> Function(Project project) activateProject;
}
