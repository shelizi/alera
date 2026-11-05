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

class _McpStubEditorWorkbenchController
    extends _McpNavigationWorkbenchController {
  _McpStubEditorWorkbenchController(super.state);

  final List<({String path, bool forceNew})> opens = [];

  @override
  Future<WorkspaceTabRecord> openEditorTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
    bool forceNew = false,
  }) async {
    opens.add((path: relativePath, forceNew: forceNew));
    final now = DateTime.utc(2026, 11, 4);
    final tab = WorkspaceTabRecord(
      id: 'mcp-stub-tab-${opens.length}',
      workspaceId: workspace.id,
      // Do not mount a native CodeForge editor in this Shell protocol test.
      kind: .terminal,
      title: 'Editor placeholder',
      createdAt: now,
      updatedAt: now,
    );
    state = state.copyWith(
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        ...state.tabsByWorkspace,
        workspace.id: <WorkspaceTabRecord>[tab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{
        ...state.layoutByWorkspace,
        workspace.id: WorkbenchLayout.single(
          workspaceId: workspace.id,
          tabIds: <String>[tab.id],
        ),
      },
    );
    return tab;
  }
}

class _McpCapturingEditorSessionRegistry extends EditorSessionRegistry {
  final List<({String tabId, WorkspaceEditorRevealTarget target})> requests =
      [];

  @override
  Future<bool> revealAndWait(
    String tabId,
    WorkspaceEditorRevealTarget target, {
    Duration timeout = const Duration(seconds: 8),
  }) async {
    requests.add((tabId: tabId, target: target));
    return true;
  }
}

void _registerAleraShellMcpNavigationTests() {
  testWidgets(
    'MCP open_file defaults to Alera and forwards caret column to the Editor',
    (tester) async {
      final state = _stackedWorkbenchState();
      final workspace = state.activeWorkspace!;
      final host = _McpNavigationRuntimeHost();
      final registry = _McpCapturingEditorSessionRegistry();
      final controller = _McpStubEditorWorkbenchController(state);
      addTearDown(host.dispose);
      addTearDown(registry.dispose);
      await _pumpShell(
        tester,
        state: state,
        controller: controller,
        editorSessionRegistry: registry,
        mcpRuntimeHostClient: host,
        mcpFilePathResolver: (_, path) async => 'lib/main.dart',
      );
      host.emitNavigation(
        id: 'internal-column',
        windowId: host.windowId!,
        workspaceId: workspace.id,
        action: 'open_file',
        arguments: <String, Object?>{
          'relativePath': 'lib/main.dart',
          'line': 120,
          'column': 8,
        },
      );
      for (
        var i = 0;
        i < 75 && host.completion('internal-column') == null;
        i++
      ) {
        await tester.pump(const Duration(milliseconds: 20));
      }
      final completion = host.completion('internal-column');
      expect(completion?['applied'], true);
      expect(completion?['tabId'], isNotNull);
      expect(controller.opens, hasLength(1));
      expect(controller.opens.single, (path: 'lib/main.dart', forceNew: false));
      expect(registry.requests, hasLength(1));
      expect(registry.requests.single.target.line, 120);
      expect(registry.requests.single.target.column, 8);
      expect(registry.requests.single.target.matchLength, 0);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
    },
  );

  testWidgets(
    'MCP launches explicit Zed/VS Code at column without creating Alera tabs',
    (tester) async {
      final rootPath = p.join(Directory.systemTemp.path, 'mcp-ext-editor-test');
      final filePath = p.join(rootPath, 'lib', 'main.dart');
      final original = _stackedWorkbenchState();
      final now = DateTime.utc(2026, 11, 4);
      final project = Project(
        id: 'project-ext',
        name: 'External',
        repoPath: rootPath,
        createdAt: now,
        updatedAt: now,
      );
      final workspace = Workspace(
        id: 'workspace-ext',
        projectId: project.id,
        name: 'Main',
        path: rootPath,
        createdAt: now,
        updatedAt: now,
        kind: .main,
        status: .active,
      );
      final state = original.copyWith(
        projects: <Project>[...original.projects, project],
        workspacesByProject: <String, List<Workspace>>{
          ...original.workspacesByProject,
          project.id: <Workspace>[workspace],
        },
      );
      final host = _McpNavigationRuntimeHost();
      final zed = _McpFakeExternalEditor();
      final vscode = _McpFakeExternalEditor();
      addTearDown(host.dispose);
      final controller = _McpNavigationWorkbenchController(state);
      await _pumpShell(
        tester,
        state: state,
        controller: controller,
        mcpRuntimeHostClient: host,
        mcpFilePathResolver: (target, rawPath) async {
          expect(target.id, workspace.id);
          expect(rawPath, 'lib/main.dart');
          return 'lib/main.dart';
        },
        mcpExternalEditorLaunchers:
            <ExternalEditorKind, ExternalEditorLauncher>{
              ExternalEditorKind.zed: zed,
              ExternalEditorKind.vscode: vscode,
            },
      );
      final window = host.windowId!;

      Future<Map<String, Object?>> invoke(
        String id,
        String editor, {
        int? line = 120,
        int? column = 8,
        String? openMode,
      }) async {
        host.emitNavigation(
          id: id,
          windowId: window,
          workspaceId: workspace.id,
          action: 'open_file',
          arguments: <String, Object?>{
            'projectId': project.id,
            'workspaceId': workspace.id,
            'relativePath': 'lib/main.dart',
            'editor': editor,
            if (line != null) 'line': line,
            if (column != null) 'column': column,
            if (openMode != null) 'openMode': openMode,
          },
        );
        for (var i = 0; i < 75; i++) {
          await tester.pump(const Duration(milliseconds: 20));
          final completion = host.completion(id);
          if (completion != null) return completion;
        }
        fail(
          'Missing MCP editor acknowledgment: $id; '
          'runtimeRequests=${host.requests.map((request) => request.type).toList()}, '
          'zedLaunches=${zed.requests.length}, vscodeLaunches=${vscode.requests.length}',
        );
      }

      final vscodeReceipt = await invoke('vs-success', 'vscode');
      expect(vscodeReceipt['applied'], true);
      expect(vscodeReceipt['tabId'], isNull);
      expect(vscode.requests, hasLength(1));
      expect(vscode.requests.single.workspacePath, rootPath);
      expect(
        p.normalize(vscode.requests.single.filePath),
        p.normalize(filePath),
      );
      expect(vscode.requests.single.line, 120);
      expect(vscode.requests.single.column, 8);
      expect(
        controller.state.activeWorkspaceId,
        original.activeWorkspaceId,
        reason: 'External launch must not switch Alera Workspace',
      );
      expect(
        controller.state.tabsByWorkspace[workspace.id],
        isNull,
        reason: 'External launch must not create an internal Editor Tab',
      );

      final zedReceipt = await invoke('zed-success', 'zed', line: 3, column: 2);
      expect(zedReceipt['applied'], true);
      expect(zed.requests.single.line, 3);
      expect(zed.requests.single.column, 2);

      vscode.installed = false;
      final absent = await invoke('vs-unavailable', 'vscode');
      expect(absent['applied'], false);
      expect(absent['errorCode'], 'external_editor_unavailable');
      expect(vscode.requests, hasLength(1));

      vscode.installed = true;
      vscode.result = externalEditorLaunchFailure(
        ExternalEditorLaunchFailureKind.failed,
        'Launch failed',
      );
      final failed = await invoke('vs-failed', 'vscode');
      expect(failed['applied'], false);
      expect(failed['errorCode'], 'external_editor_failed');

      final invalid = await invoke('invalid-editor', 'arbitrary-editor');
      expect(invalid['applied'], false);
      expect(invalid['errorCode'], 'invalid_editor');
      final missingLine = await invoke('missing-line', 'zed', line: null);
      expect(missingLine['applied'], false);
      expect(missingLine['errorCode'], 'invalid_position');
      final wrongMode = await invoke('wrong-mode', 'zed', openMode: 'new');
      expect(wrongMode['applied'], false);
      expect(wrongMode['errorCode'], 'invalid_editor');
      expect(zed.requests, hasLength(1));

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
    },
  );

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
