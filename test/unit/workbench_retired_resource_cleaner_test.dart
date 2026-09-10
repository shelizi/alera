import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/application/workbench_retired_resource_cleaner.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('releaseTabs releases runtime, editor, and terminal observers', () {
    final lifecycle = _FakeTerminalRuntimeLifecycle();
    final forgotten = <String>[];
    final clearedTerminalSessions = <String>[];
    final cleaner = WorkbenchRetiredResourceCleaner(
      runtimeLifecycle: lifecycle,
      forgetEditorSession: forgotten.add,
      clearTerminalSession: clearedTerminalSessions.add,
    );

    cleaner.releaseTabs(<WorkspaceTabRecord>[
      _tab('terminal', sessionId: 'session-1'),
      _tab('editor', kind: WorkspaceTabKind.editor),
    ]);

    expect(lifecycle.releasedTabIds, <String>['terminal', 'editor']);
    expect(forgotten, <String>['terminal', 'editor']);
    expect(clearedTerminalSessions, <String>['session-1']);
  });

  test(
    'releaseWorkspace releases the runtime once and cleans tab resources',
    () {
      final lifecycle = _FakeTerminalRuntimeLifecycle();
      final forgotten = <String>[];
      final clearedTerminalSessions = <String>[];
      final cleaner = WorkbenchRetiredResourceCleaner(
        runtimeLifecycle: lifecycle,
        forgetEditorSession: forgotten.add,
        clearTerminalSession: clearedTerminalSessions.add,
      );

      cleaner.releaseWorkspace('workspace', <WorkspaceTabRecord>[
        _tab('terminal', sessionId: 'session-1'),
        _tab('editor', kind: WorkspaceTabKind.editor),
      ]);

      expect(lifecycle.releasedWorkspaceIds, <String>['workspace']);
      expect(lifecycle.releasedTabIds, isEmpty);
      expect(forgotten, <String>['terminal', 'editor']);
      expect(clearedTerminalSessions, <String>['session-1']);
    },
  );
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
  final releasedTabIds = <String>[];
  final releasedWorkspaceIds = <String>[];

  @override
  void closeTab(String tabId) {}

  @override
  void closeWorkspace(String workspaceId) {}

  @override
  void releaseTab(String tabId) => releasedTabIds.add(tabId);

  @override
  void releaseWorkspace(String workspaceId) =>
      releasedWorkspaceIds.add(workspaceId);
}
