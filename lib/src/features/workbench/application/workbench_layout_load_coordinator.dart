import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchLayoutLoadCoordinator {
  final Set<String> _loadingWorkspaceIds = <String>{};

  Future<void> load({
    required String workspaceId,
    required Future<List<WorkspaceTabRecord>> Function(String workspaceId)
    listTabs,
    required Future<WorkbenchLayout> Function({
      required String workspaceId,
      required List<WorkspaceTabRecord> tabs,
    })
    resolveLayout,
    required Future<void> Function(WorkbenchLayout layout) applyLayout,
    required void Function(Object error) onError,
  }) async {
    if (!_loadingWorkspaceIds.add(workspaceId)) {
      return;
    }
    try {
      final tabs = await listTabs(workspaceId);
      final layout = await resolveLayout(workspaceId: workspaceId, tabs: tabs);
      await applyLayout(layout);
    } catch (error) {
      onError(error);
    } finally {
      _loadingWorkspaceIds.remove(workspaceId);
    }
  }
}
