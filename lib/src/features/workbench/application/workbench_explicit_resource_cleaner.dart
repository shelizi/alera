import 'dart:async';

import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchExplicitResourceIdConsumer = void Function(String id);
typedef WorkbenchExplicitAsyncResourceIdConsumer = Future<void> Function(
  String id,
);

/// Cleans client-local resources for explicit Workbench close operations.
///
/// Explicit close differs from persisted-state retirement: the runtime is
/// actively terminated here. Workspace deletion uses two phases so the
/// controller can preserve the hosted-review failure ordering between local
/// resource teardown and observer cleanup.
abstract interface class WorkbenchExplicitTabResourceCleaner {
  void closeTabLocalResources(String tabId);
}

abstract interface class WorkbenchExplicitWorkspaceResourceCleaner {
  void closeWorkspaceLocalResources(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  );

  void clearDeletedWorkspaceObservers(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  );
}

final class WorkbenchExplicitResourceCleaner
    implements
        WorkbenchExplicitTabResourceCleaner,
        WorkbenchExplicitWorkspaceResourceCleaner {
  const WorkbenchExplicitResourceCleaner({
    required TerminalRuntimeLifecycle runtimeLifecycle,
    required WorkbenchExplicitResourceIdConsumer forgetEditorSession,
    required WorkbenchExplicitResourceIdConsumer removeWorkspaceActivity,
    required WorkbenchExplicitResourceIdConsumer clearAgentWorkspace,
    WorkbenchExplicitResourceIdConsumer? clearTerminalSession,
    WorkbenchExplicitAsyncResourceIdConsumer? clearTerminalOverlays,
  }) : _runtimeLifecycle = runtimeLifecycle,
       _forgetEditorSession = forgetEditorSession,
       _removeWorkspaceActivity = removeWorkspaceActivity,
       _clearAgentWorkspace = clearAgentWorkspace,
       _clearTerminalSession = clearTerminalSession,
       _clearTerminalOverlays = clearTerminalOverlays;

  final TerminalRuntimeLifecycle _runtimeLifecycle;
  final WorkbenchExplicitResourceIdConsumer _forgetEditorSession;
  final WorkbenchExplicitResourceIdConsumer _removeWorkspaceActivity;
  final WorkbenchExplicitResourceIdConsumer _clearAgentWorkspace;
  final WorkbenchExplicitResourceIdConsumer? _clearTerminalSession;
  final WorkbenchExplicitAsyncResourceIdConsumer? _clearTerminalOverlays;

  void closeTabLocalResources(String tabId) {
    _runtimeLifecycle.closeTab(tabId);
    _forgetEditorSession(tabId);
  }

  void closeWorkspaceLocalResources(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    _runtimeLifecycle.closeWorkspace(workspaceId);
    for (final tab in tabs) {
      _forgetEditorSession(tab.id);
    }
  }

  void clearDeletedWorkspaceObservers(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    _removeWorkspaceActivity(workspaceId);
    _clearAgentWorkspace(workspaceId);
    for (final tab in tabs) {
      if (tab.kind != WorkspaceTabKind.terminal) {
        continue;
      }
      final sessionId = tab.terminalSessionId;
      if (sessionId.isEmpty) {
        continue;
      }
      _clearTerminalSession?.call(sessionId);
      final clearTerminalOverlays = _clearTerminalOverlays;
      if (clearTerminalOverlays != null) {
        unawaited(clearTerminalOverlays(sessionId).catchError((Object _) {}));
      }
    }
  }
}
