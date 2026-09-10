import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchResourceIdConsumer = void Function(String id);

/// Releases client-local resources after persisted Workbench state retires a
/// tab or workspace without issuing another process termination request.
final class WorkbenchRetiredResourceCleaner {
  const WorkbenchRetiredResourceCleaner({
    required TerminalRuntimeLifecycle runtimeLifecycle,
    required WorkbenchResourceIdConsumer forgetEditorSession,
    WorkbenchResourceIdConsumer? clearTerminalSession,
  }) : _runtimeLifecycle = runtimeLifecycle,
       _forgetEditorSession = forgetEditorSession,
       _clearTerminalSession = clearTerminalSession;

  final TerminalRuntimeLifecycle _runtimeLifecycle;
  final WorkbenchResourceIdConsumer _forgetEditorSession;
  final WorkbenchResourceIdConsumer? _clearTerminalSession;

  void releaseTabs(Iterable<WorkspaceTabRecord> tabs) {
    for (final tab in tabs) {
      _runtimeLifecycle.releaseTab(tab.id);
      _releaseTabLocalResources(tab);
    }
  }

  void releaseWorkspace(String workspaceId, Iterable<WorkspaceTabRecord> tabs) {
    _runtimeLifecycle.releaseWorkspace(workspaceId);
    for (final tab in tabs) {
      _releaseTabLocalResources(tab);
    }
  }

  void _releaseTabLocalResources(WorkspaceTabRecord tab) {
    _forgetEditorSession(tab.id);
    if (tab.kind == WorkspaceTabKind.terminal) {
      _clearTerminalSession?.call(tab.terminalSessionId);
    }
  }
}
