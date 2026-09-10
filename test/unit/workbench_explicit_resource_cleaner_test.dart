import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('closeWorkspaceLocalResources closes runtime and forgets every editor session', () {
    final lifecycle = _FakeTerminalRuntimeLifecycle();
    final forgotten = <String>[];
    final activity = <String>[];
    final agentWorkspaces = <String>[];
    final terminalSessions = <String>[];
    final overlays = <String>[];
    final cleaner = WorkbenchExplicitResourceCleaner(
      runtimeLifecycle: lifecycle,
      forgetEditorSession: forgotten.add,
      removeWorkspaceActivity: activity.add,
      clearAgentWorkspace: agentWorkspaces.add,
      clearTerminalSession: terminalSessions.add,
      clearTerminalOverlays: (id) async => overlays.add(id),
    );

    cleaner.closeWorkspaceLocalResources('workspace', <WorkspaceTabRecord>[
      _tab('terminal', sessionId: 'session-1'),
      _tab('editor', kind: WorkspaceTabKind.editor),
    ]);

    expect(lifecycle.closedWorkspaceIds, <String>['workspace']);
    expect(forgotten, <String>['terminal', 'editor']);
    expect(activity, isEmpty);
    expect(agentWorkspaces, isEmpty);
    expect(terminalSessions, isEmpty);
    expect(overlays, isEmpty);
  });

  test(
    'closeTabLocalResources closes one runtime tab and forgets its editor',
    () {
      final lifecycle = _FakeTerminalRuntimeLifecycle();
      final forgotten = <String>[];
      final cleaner = WorkbenchExplicitResourceCleaner(
        runtimeLifecycle: lifecycle,
        forgetEditorSession: forgotten.add,
        removeWorkspaceActivity: (_) {},
        clearAgentWorkspace: (_) {},
      );

      cleaner.closeTabLocalResources('tab-1');

      expect(lifecycle.closedTabIds, <String>['tab-1']);
      expect(forgotten, <String>['tab-1']);
    },
  );

  test('clearDeletedWorkspaceObservers clears workspace state and terminal-only observers', () async {
    final lifecycle = _FakeTerminalRuntimeLifecycle();
    final forgotten = <String>[];
    final activity = <String>[];
    final agentWorkspaces = <String>[];
    final terminalSessions = <String>[];
    final overlays = <String>[];
    final cleaner = WorkbenchExplicitResourceCleaner(
      runtimeLifecycle: lifecycle,
      forgetEditorSession: forgotten.add,
      removeWorkspaceActivity: activity.add,
      clearAgentWorkspace: agentWorkspaces.add,
      clearTerminalSession: terminalSessions.add,
      clearTerminalOverlays: (id) async => overlays.add(id),
    );

    cleaner.clearDeletedWorkspaceObservers('workspace', <WorkspaceTabRecord>[
      _tab('terminal', sessionId: 'session-1'),
      _tab('editor', kind: WorkspaceTabKind.editor),
    ]);
    await Future<void>.delayed(Duration.zero);

    expect(lifecycle.closedWorkspaceIds, isEmpty);
    expect(forgotten, isEmpty);
    expect(activity, <String>['workspace']);
    expect(agentWorkspaces, <String>['workspace']);
    expect(terminalSessions, <String>['session-1']);
    expect(overlays, <String>['session-1']);
  });
}

WorkspaceTabRecord _tab(
  String id, {
  WorkspaceTabKind kind = WorkspaceTabKind.terminal,
  String? sessionId,
}) {
  final now = DateTime.utc(2026, 9, 10);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    kind: kind,
    title: id,
    createdAt: now,
    updatedAt: now,
    payload: sessionId == null
        ? const <String, Object?>{}
        : <String, Object?>{workspaceTabTerminalSessionIdPayloadKey: sessionId},
  );
}

final class _FakeTerminalRuntimeLifecycle implements TerminalRuntimeLifecycle {
  final closedWorkspaceIds = <String>[];
  final closedTabIds = <String>[];

  @override
  void closeTab(String tabId) => closedTabIds.add(tabId);

  @override
  void closeWorkspace(String workspaceId) =>
      closedWorkspaceIds.add(workspaceId);

  @override
  void releaseTab(String tabId) {}

  @override
  void releaseWorkspace(String workspaceId) {}
}
