import 'package:alera/src/features/workbench/application/terminal_runtime_focus.dart';
import 'package:alera/src/features/workbench/application/workbench_sidebar_terminal_selection_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'selects the workspace tab before focusing the fresh terminal record',
    () async {
      final events = <String>[];
      final oldTab = _tab('tab', title: 'old');
      final freshTab = _tab('tab', title: 'fresh');
      var tabs = <WorkspaceTabRecord>[oldTab];
      final focus = _RecordingTerminalFocus(events);
      final coordinator = WorkbenchSidebarTerminalSelectionCoordinator(
        selectWorkspaceTab: ({required workspaceId, required tabId}) async {
          events.add('select:$workspaceId:$tabId');
          tabs = <WorkspaceTabRecord>[freshTab];
        },
        readWorkspaceTabs: (workspaceId) {
          events.add('read:$workspaceId');
          return tabs;
        },
        terminalFocus: focus,
      );

      await coordinator.select(workspace: _workspace(), tabId: oldTab.id);

      expect(events, <String>[
        'select:workspace:tab',
        'read:workspace',
        'focus:fresh',
      ]);
      expect(focus.lastTab, same(freshTab));
    },
  );

  test(
    'does not focus a terminal removed while selection is in flight',
    () async {
      final events = <String>[];
      var tabs = <WorkspaceTabRecord>[_tab('tab')];
      final focus = _RecordingTerminalFocus(events);
      final coordinator = WorkbenchSidebarTerminalSelectionCoordinator(
        selectWorkspaceTab: ({required workspaceId, required tabId}) async {
          events.add('select');
          tabs = const <WorkspaceTabRecord>[];
        },
        readWorkspaceTabs: (_) => tabs,
        terminalFocus: focus,
      );

      await coordinator.select(workspace: _workspace(), tabId: 'tab');

      expect(events, <String>['select']);
      expect(focus.lastTab, isNull);
    },
  );

  test('selection failure stops before reading tabs or focusing', () async {
    final events = <String>[];
    final focus = _RecordingTerminalFocus(events);
    final coordinator = WorkbenchSidebarTerminalSelectionCoordinator(
      selectWorkspaceTab: ({required workspaceId, required tabId}) async {
        events.add('select');
        throw StateError('selection failed');
      },
      readWorkspaceTabs: (_) {
        events.add('read');
        return <WorkspaceTabRecord>[_tab('tab')];
      },
      terminalFocus: focus,
    );

    await expectLater(
      coordinator.select(workspace: _workspace(), tabId: 'tab'),
      throwsStateError,
    );

    expect(events, <String>['select']);
    expect(focus.lastTab, isNull);
  });
}

Workspace _workspace() => Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: 'workspace-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.main,
  status: WorkspaceStatus.active,
);

WorkspaceTabRecord _tab(String id, {String? title}) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: title ?? id,
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

final class _RecordingTerminalFocus implements TerminalRuntimeFocus {
  _RecordingTerminalFocus(this.events);

  final List<String> events;
  WorkspaceTabRecord? lastTab;

  @override
  void requestFocus({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    lastTab = tab;
    events.add('focus:${tab.title}');
  }
}
