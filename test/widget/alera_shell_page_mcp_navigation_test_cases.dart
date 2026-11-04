part of 'alera_shell_page_test.dart';

class const _McpNavigationTitleSource() implements TerminalSessionTitleSource {
  @override
  TerminalSessionTitleSnapshot? peekTitleForTab(String tabId) => null;
}

/// Test UI acknowledgments, not just Host command admission. A successful
/// response must reflect the selected visible Workspace / Tab.
class _McpNavigationRuntimeHost implements RuntimeHostClient {
  final StreamController<RuntimeHostEvent> _events =
      StreamController<RuntimeHostEvent>.broadcast(sync: true);
  final List<({String type, Map<String, Object?> payload})> requests =
      <({String type, Map<String, Object?> payload})>[];

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => _events.stream;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    requests.add((type: type, payload: Map<String, Object?>.of(payload)));
    return <String, Object?>{};
  }

  String? get windowId {
    for (final request in requests) {
      if (request.type == 'uiControl.register') {
        return request.payload['windowId'] as String?;
      }
    }
    return null;
  }

  void emitNavigation({
    required String id,
    required String windowId,
    required String workspaceId,
    required String action,
    required Map<String, Object?> arguments,
  }) {
    _events.add(
      RuntimeHostEvent('mcpUiCommand', <String, Object?>{
        'requestId': id,
        'windowId': windowId,
        'workspaceId': workspaceId,
        'action': action,
        'arguments': arguments,
      }),
    );
  }

  Map<String, Object?>? completion(String id) {
    for (final request in requests.reversed) {
      if (request.type == 'uiControl.complete' &&
          request.payload['requestId'] == id) {
        return request.payload;
      }
    }
    return null;
  }

  Future<void> dispose() => _events.close();
}

class _McpNavigationWorkbenchController extends _ShellTestWorkbenchController {
  _McpNavigationWorkbenchController(super.state);

  final List<String> navigatedWorkspaceIds = <String>[];

  @override
  Future<void> selectWorkspaceWithoutTerminal({
    required Project project,
    required Workspace workspace,
  }) async {
    navigatedWorkspaceIds.add(workspace.id);
    await selectWorkspace(project: project, workspace: workspace);
  }
}

void _registerAleraShellMcpNavigationTests() {
  testWidgets(
    'MCP cross-project navigation ACK follows visible Workspace and Tab',
    (tester) async {
      final now = DateTime.utc(2026, 11, 4);
      final original = _stackedWorkbenchState();
      final project = Project(
        id: 'project-b',
        name: 'Care',
        repoPath: '/repo/care',
        createdAt: now,
        updatedAt: now,
      );
      final workspace = Workspace(
        id: 'workspace-b',
        projectId: project.id,
        name: 'Main',
        path: project.repoPath,
        createdAt: now,
        updatedAt: now,
        kind: .main,
        status: .active,
      );
      final tab = WorkspaceTabRecord(
        id: 'tab-b',
        workspaceId: workspace.id,
        kind: .terminal,
        title: 'Care Terminal',
        createdAt: now,
        updatedAt: now,
      );
      final state = original.copyWith(
        projects: <Project>[...original.projects, project],
        workspacesByProject: <String, List<Workspace>>{
          ...original.workspacesByProject,
          project.id: <Workspace>[workspace],
        },
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          ...original.tabsByWorkspace,
          workspace.id: <WorkspaceTabRecord>[tab],
        },
        layoutByWorkspace: <String, WorkbenchLayout>{
          ...original.layoutByWorkspace,
          workspace.id: WorkbenchLayout.single(
            workspaceId: workspace.id,
            tabIds: <String>[tab.id],
          ),
        },
        activeTabIdByWorkspace: <String, String>{
          ...original.activeTabIdByWorkspace,
          workspace.id: tab.id,
        },
      );
      final host = _McpNavigationRuntimeHost();
      addTearDown(host.dispose);
      final controller = _McpNavigationWorkbenchController(state);
      await _pumpShell(
        tester,
        state: state,
        controller: controller,
        mcpRuntimeHostClient: host,
      );
      final window = host.windowId;
      expect(window, isNotNull, reason: 'MCP window should register with Host');

      Future<Map<String, Object?>> waitForCompletion(String id) async {
        for (var i = 0; i < 60; i++) {
          await tester.pump(const Duration(milliseconds: 20));
          final completed = host.completion(id);
          if (completed != null) return completed;
        }
        fail('MCP UI never acknowledged $id');
      }

      host.emitNavigation(
        id: 'focus-b',
        windowId: window!,
        workspaceId: workspace.id,
        action: 'focus_workspace',
        arguments: <String, Object?>{
          'workspaceId': workspace.id,
          'projectId': project.id,
        },
      );
      expect((await waitForCompletion('focus-b'))['applied'], true);
      expect(controller.state.activeProjectId, project.id);
      expect(controller.state.activeWorkspaceId, workspace.id);
      expect(controller.navigatedWorkspaceIds, <String>[workspace.id]);

      // Return to the source workspace, then select an existing foreign Tab in
      // one operation instead of relying on the earlier workspace focus.
      await controller.selectWorkspace(
        project: original.projects.single,
        workspace: original.workspacesByProject.values.single.single,
      );
      await tester.pump();
      host.emitNavigation(
        id: 'tab-b',
        windowId: window,
        workspaceId: workspace.id,
        action: 'select_tab',
        arguments: <String, Object?>{
          'workspaceId': workspace.id,
          'projectId': project.id,
          'tabId': tab.id,
        },
      );
      final selected = await waitForCompletion('tab-b');
      expect(selected['applied'], true);
      expect(selected['tabId'], tab.id);
      expect(controller.state.activeWorkspaceId, workspace.id);
      expect(controller.state.activeWorkspaceTab?.id, tab.id);

      host.emitNavigation(
        id: 'bad-project',
        windowId: window,
        workspaceId: workspace.id,
        action: 'focus_workspace',
        arguments: <String, Object?>{
          'workspaceId': workspace.id,
          'projectId': 'forged-project',
        },
      );
      final rejected = await waitForCompletion('bad-project');
      expect(rejected['applied'], false);
      expect(rejected['errorCode'], 'workspace_missing');

      host.emitNavigation(
        id: 'bad-tab',
        windowId: window,
        workspaceId: workspace.id,
        action: 'select_tab',
        arguments: <String, Object?>{
          'workspaceId': workspace.id,
          'projectId': project.id,
          'tabId': 'unlisted-tab',
        },
      );
      final missing = await waitForCompletion('bad-tab');
      expect(missing['applied'], false);
      expect(missing['errorCode'], 'tab_missing');

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
    },
  );
}
