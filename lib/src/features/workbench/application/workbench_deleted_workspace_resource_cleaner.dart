import 'dart:async';

import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchDeletedResourceIdConsumer = void Function(String id);
typedef WorkbenchDeletedAsyncResourceIdConsumer = Future<void> Function(
  String id,
);

/// Cleans client-local resources for an explicitly deleted workspace.
///
/// Explicit deletion differs from persisted-state retirement: the runtime is
/// actively closed here because the managed workspace removal has already
/// stopped its process trees. The two phases let the controller preserve the
/// existing hosted-review failure ordering between local resource teardown and
/// observer cleanup.
final class WorkbenchDeletedWorkspaceResourceCleaner {
  const WorkbenchDeletedWorkspaceResourceCleaner({
    required TerminalRuntimeLifecycle runtimeLifecycle,
    required WorkbenchDeletedResourceIdConsumer forgetEditorSession,
    required WorkbenchDeletedResourceIdConsumer removeWorkspaceActivity,
    required WorkbenchDeletedResourceIdConsumer clearAgentWorkspace,
    WorkbenchDeletedResourceIdConsumer? clearTerminalSession,
    WorkbenchDeletedAsyncResourceIdConsumer? clearTerminalOverlays,
  }) : _runtimeLifecycle = runtimeLifecycle,
       _forgetEditorSession = forgetEditorSession,
       _removeWorkspaceActivity = removeWorkspaceActivity,
       _clearAgentWorkspace = clearAgentWorkspace,
       _clearTerminalSession = clearTerminalSession,
       _clearTerminalOverlays = clearTerminalOverlays;

  final TerminalRuntimeLifecycle _runtimeLifecycle;
  final WorkbenchDeletedResourceIdConsumer _forgetEditorSession;
  final WorkbenchDeletedResourceIdConsumer _removeWorkspaceActivity;
  final WorkbenchDeletedResourceIdConsumer _clearAgentWorkspace;
  final WorkbenchDeletedResourceIdConsumer? _clearTerminalSession;
  final WorkbenchDeletedAsyncResourceIdConsumer? _clearTerminalOverlays;

  void closeLocalResources(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    _runtimeLifecycle.closeWorkspace(workspaceId);
    for (final tab in tabs) {
      _forgetEditorSession(tab.id);
    }
  }

  void clearObservers(String workspaceId, Iterable<WorkspaceTabRecord> tabs) {
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
