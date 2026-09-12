part of 'workbench_controller.dart';

/// Facade over the catalog owner: workspace section CRUD and sidebar
/// section-collapsed preferences.
mixin _WorkbenchControllerSections
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<List<WorkspaceSection>> listWorkspaceSections() =>
      _catalogOwner.listWorkspaceSections();

  Future<void> saveWorkspaceSection(
    String workspaceId, {
    String? sectionId,
    String? newName,
  }) => _catalogOwner.saveWorkspaceSection(
    workspaceId,
    sectionId: sectionId,
    newName: newName,
  );

  Future<void> deleteWorkspaceSection(String sectionId) =>
      _catalogOwner.deleteWorkspaceSection(sectionId);

  void setSectionSort(WorkbenchSortBy sort) =>
      _catalogOwner.setSectionSort(sort);

  void toggleSectionCollapsed(String? sectionId) =>
      _catalogOwner.toggleSectionCollapsed(sectionId);
}
