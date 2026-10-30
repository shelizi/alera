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

  /// Tab watcher removal is not proof of terminal exit. A cross-workspace
  /// move removes the source Tab but keeps the same live PTY and Codex
  /// transcript watcher. Clear that per-session watcher only after a caller
  /// has independently confirmed the Session really ended.
  void releaseTabs(
    Iterable<WorkspaceTabRecord> tabs, {
    bool confirmedSessionExit = false,
  }) {
    for (final tab in tabs) {
      _runtimeLifecycle.releaseTab(tab.id);
      _releaseTabLocalResources(
        tab,
        confirmedSessionExit: confirmedSessionExit,
      );
    }
  }

  void releaseWorkspace(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs, {
    bool confirmedSessionExit = false,
  }) {
    _runtimeLifecycle.releaseWorkspace(workspaceId);
    for (final tab in tabs) {
      _releaseTabLocalResources(
        tab,
        confirmedSessionExit: confirmedSessionExit,
      );
    }
  }

  void _releaseTabLocalResources(
    WorkspaceTabRecord tab, {
    required bool confirmedSessionExit,
  }) {
    _forgetEditorSession(tab.id);
    if (confirmedSessionExit && tab.kind == WorkspaceTabKind.terminal) {
      _clearTerminalSession?.call(tab.terminalSessionId);
    }
  }
}
