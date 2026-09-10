import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

abstract interface class WorkbenchWorkspaceSelectionTabStore {
  Future<WorkspaceTabRecord> ensureInitialTerminalTab(String workspaceId);

  Future<List<WorkspaceTabRecord>> listTabs(String workspaceId);
}

abstract interface class WorkbenchWorkspaceSelectionLayoutResolver {
  Future<WorkbenchLayout> resolve({
    required String workspaceId,
    required List<WorkspaceTabRecord> tabs,
  });
}

final class WorkbenchWorkspaceSelectionHydration {
  const WorkbenchWorkspaceSelectionHydration({
    required this.tabs,
    required this.layout,
  });

  final List<WorkspaceTabRecord> tabs;
  final WorkbenchLayout layout;
}

final class WorkbenchWorkspaceSelectionHydrator {
  const WorkbenchWorkspaceSelectionHydrator({
    required WorkbenchWorkspaceSelectionTabStore tabStore,
    required WorkbenchWorkspaceSelectionLayoutResolver layoutResolver,
  }) : _tabStore = tabStore,
       _layoutResolver = layoutResolver;

  final WorkbenchWorkspaceSelectionTabStore _tabStore;
  final WorkbenchWorkspaceSelectionLayoutResolver _layoutResolver;

  Future<WorkbenchWorkspaceSelectionHydration> hydrate({
    required String workspaceId,
    required bool ensureInitialTerminal,
  }) async {
    if (ensureInitialTerminal) {
      await _tabStore.ensureInitialTerminalTab(workspaceId);
    }
    final tabs = await _tabStore.listTabs(workspaceId);
    final layout = await _layoutResolver.resolve(
      workspaceId: workspaceId,
      tabs: tabs,
    );
    return WorkbenchWorkspaceSelectionHydration(tabs: tabs, layout: layout);
  }
}
