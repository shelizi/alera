part of 'alera_shell_page.dart';

/// UI operations from the Alera MCP Host. Only the registered primary Shell
/// consumes its window-specific events, and the Host sees `applied` only after
/// the Workbench has actually selected the target Tab/Explorer row.
extension _AleraShellMcpUi on _AleraShellPageBodyState {
  void _startMcpUiBridge() {
    if (_mcpUiSubscription != null) return;
    final host = ref.read(runtimeHostClientProvider);
    _mcpUiSubscription = host.runtimeEvents.listen((event) {
      if (!mounted) return;
      if (event.name == aleraRuntimeHostConnectedEvent) {
        unawaited(_registerMcpUiWindow());
        return;
      }
      if (event.name != 'mcpUiCommand' ||
          event.payload['windowId'] != _mcpWindowId) {
        return;
      }
      // Only one command per window at a time. The Host imposes an independent
      // deadline, so a queued command cannot imply successful visual control.
      _mcpUiQueue = _mcpUiQueue
          .catchError((Object _, StackTrace _) {})
          .then((_) => _handleMcpUiCommand(event.payload));
    });
    unawaited(_registerMcpUiWindow());
  }

  Future<void> _registerMcpUiWindow() async {
    if (!mounted) return;
    try {
      await ref.read(runtimeHostClientProvider).runtimeRequest(
        'uiControl.register',
        <String, Object?>{'windowId': _mcpWindowId},
      );
    } catch (_) {
      // Compatible with older Hosts; this is an opt-in MCP alpha.
      // No UI tool can succeed without a current registered window.
      Logger('AleraMcpUi').fine('MCP UI registration is not available.');
    }
  }

  Future<void> _handleMcpUiCommand(Map<String, Object?> command) async {
    final id = command['requestId'];
    if (id is! String || id.isEmpty || !mounted) return;
    String? tabId;
    String? createdWorkspaceId;
    String? errorCode;
    Object? settingValue;
    try {
      if (command['action'] == 'get_setting' ||
          command['action'] == 'update_setting') {
        settingValue = await _applyMcpSettingCommand(command);
      } else if (command['action'] == 'open_settings') {
        await _mcpOpenSettingsDialog(command);
      } else if (command['action'] == 'create_workspace') {
        createdWorkspaceId = await _mcpCreateWorkspace(command);
      } else {
        tabId = await _applyMcpUiCommand(command);
      }
    } on _McpUiFailure catch (error) {
      errorCode = error.code;
    } catch (_) {
      errorCode = 'ui_operation_failed';
    }
    if (!mounted) return;
    try {
      await ref.read(runtimeHostClientProvider).runtimeRequest(
        'uiControl.complete',
        <String, Object?>{
          'requestId': id,
          'applied': errorCode == null,
          if (tabId != null && errorCode == null) 'tabId': tabId,
          if (createdWorkspaceId != null && errorCode == null)
            'createdWorkspaceId': createdWorkspaceId,
          if (settingValue != null && errorCode == null)
            'settingValue': settingValue,
          'errorCode': ?errorCode,
        },
      );
    } catch (_) {
      // A disconnected Host or expired request must not be reported as
      // complete by the UI. The MCP caller will receive its Host-side error.
      Logger('AleraMcpUi').fine('MCP UI command receipt could not be sent.');
    }
  }

  Future<String> _mcpCreateWorkspace(Map<String, Object?> command) async {
    if (!mounted || command['windowId'] != _mcpWindowId) {
      throw const _McpUiFailure('ui_not_ready');
    }
    final args = command['arguments'];
    if (args is! Map ||
        args['projectId'] is! String ||
        args['sourceBranch'] is! String ||
        args['newBranchName'] is! String) {
      throw const _McpUiFailure('workspace_create_failed');
    }
    final projectId = args['projectId'] as String;
    final before = ref.read(workbenchControllerProvider);
    final projects = before.projects.where(
      (project) => project.id == projectId,
    );
    if (projects.length != 1) {
      throw const _McpUiFailure('workspace_missing');
    }
    final project = projects.single;
    final source = args['sourceBranch'] as String;
    final branch = args['newBranchName'] as String;
    if (source.isEmpty || branch.isEmpty) {
      throw const _McpUiFailure('workspace_create_failed');
    }
    try {
      // Existing Workbench workflow owns Git worktree creation, the catalog,
      // initial tabs and deferred setup. Do not recreate it in the MCP bridge.
      final result = await ref
          .read(workbenchControllerProvider.notifier)
          .createWorkspace(
            project: project,
            sourceBranch: source,
            newBranchName: branch,
            reuseExistingBranch: args['reuseExistingBranch'] == true,
            name: args['name'] as String?,
            parentWorkspaceId: args['parentWorkspaceId'] as String?,
          );
      if (!mounted) throw const _McpUiFailure('ui_not_ready');
      final current = ref.read(workbenchControllerProvider);
      if (result.workspace.projectId != projectId ||
          current.activeWorkspaceId != result.workspace.id ||
          !result.workspace.isActive ||
          result.workspace.isArchived ||
          !(current.workspacesByProject[projectId] ?? const <Workspace>[]).any(
            (workspace) => workspace.id == result.workspace.id,
          )) {
        throw const _McpUiFailure('ui_not_ready');
      }
      return result.workspace.id;
    } on _McpUiFailure {
      rethrow;
    } catch (_) {
      throw const _McpUiFailure('workspace_create_failed');
    }
  }

  Future<Object> _applyMcpSettingCommand(Map<String, Object?> command) async {
    if (!mounted || command['windowId'] != _mcpWindowId) {
      throw const _McpUiFailure('ui_not_ready');
    }
    final args = command['arguments'];
    if (args is! Map || args['path'] is! String) {
      throw const _McpUiFailure('setting_invalid');
    }
    final path = args['path'] as String;
    final controller = ref.read(settingsControllerProvider.notifier);
    try {
      if (command['action'] == 'get_setting') {
        // The MCP caller needs the committed Host value, not this window's
        // potentially stale in-memory snapshot from an async event watcher.
        return await controller.readMcpSettingFresh(path);
      }
      if (!args.containsKey('expectedValue') || !args.containsKey('value')) {
        throw const _McpUiFailure('setting_invalid');
      }
      final actual = await controller.updateMcpSetting(
        path: path,
        expectedValue: args['expectedValue'],
        value: args['value'],
      );
      if (!mounted ||
          mcpSettingValue(ref.read(settingsControllerProvider), path) !=
              actual) {
        throw const _McpUiFailure('ui_not_ready');
      }
      return actual;
    } on ArgumentError {
      throw const _McpUiFailure('setting_invalid');
    } on StateError catch (error) {
      if (error.toString().contains('mcp_setting_conflict')) {
        throw const _McpUiFailure('setting_conflict');
      }
      rethrow;
    }
  }

  Future<void> _mcpOpenSettingsDialog(Map<String, Object?> command) async {
    if (!mounted || command['windowId'] != _mcpWindowId) {
      throw const _McpUiFailure('ui_not_ready');
    }
    final args = command['arguments'];
    final section = args is Map ? args['sectionId'] : null;
    if (section is! String ||
        !<String>{
          'application',
          'terminal',
          'editor',
          'agents',
          'agentProfiles',
        }.contains(section)) {
      throw const _McpUiFailure('setting_invalid');
    }
    unawaited(openSettingsDialog(context, initialSectionId: section));
    await WidgetsBinding.instance.endOfFrame;
    if (!mounted) throw const _McpUiFailure('ui_not_ready');
  }

  Future<String?> _applyMcpUiCommand(Map<String, Object?> command) async {
    if (command['windowId'] != _mcpWindowId) {
      throw const _McpUiFailure('ui_not_ready');
    }
    final workspaceId = command['workspaceId'];
    final action = command['action'];
    final args = command['arguments'];
    if (workspaceId is! String || action is! String || args is! Map) {
      throw const _McpUiFailure('ui_operation_failed');
    }
    final arguments = Map<String, Object?>.from(args);
    final controller = ref.read(workbenchControllerProvider.notifier);
    final before = ref.read(workbenchControllerProvider);
    final workspaces = before.workspacesByProject.values.expand(
      (items) => items,
    );
    Workspace? workspace;
    for (final item in workspaces) {
      if (item.id == workspaceId) {
        workspace = item;
        break;
      }
    }
    if (workspace == null) {
      throw const _McpUiFailure('workspace_missing');
    }
    // The Host has already validated the durable target; the App checks that
    // the target is still visible in this window's current project catalog.
    final requestedProjectId = arguments['projectId'];
    if (!workspace.isActive ||
        workspace.isArchived ||
        (requestedProjectId != null &&
            requestedProjectId != workspace.projectId)) {
      throw const _McpUiFailure('workspace_missing');
    }
    final matchingProject = before.projects.where(
      (item) => item.id == workspace!.projectId,
    );
    if (matchingProject.isEmpty) {
      throw const _McpUiFailure('workspace_missing');
    }
    final project = matchingProject.first;
    // Explicit external opens do not change Alera's selected Workspace or Tab.
    if (action == 'open_file' &&
        arguments['editor'] != null &&
        arguments['editor'] != 'alera') {
      await _mcpOpenFileExternally(workspace, arguments);
      return null;
    }
    if (before.activeWorkspaceId != workspaceId) {
      await controller.selectWorkspaceWithoutTerminal(
        project: project,
        workspace: workspace,
      );
      if (ref.read(workbenchControllerProvider).activeWorkspaceId !=
          workspaceId) {
        throw const _McpUiFailure('ui_not_ready');
      }
    }
    if (!mounted) throw const _McpUiFailure('ui_not_ready');
    switch (action) {
      case 'focus_workspace':
        await WidgetsBinding.instance.endOfFrame;
        if (!mounted ||
            ref.read(workbenchControllerProvider).activeWorkspaceId !=
                workspaceId) {
          throw const _McpUiFailure('not_visible');
        }
        // Deliberately do not create a Terminal or change a PTY ownership.
        return null;
      case 'select_tab':
        final tabId = arguments['tabId'];
        if (tabId is! String ||
            !(ref
                        .read(workbenchControllerProvider)
                        .tabsByWorkspace[workspaceId] ??
                    const <WorkspaceTabRecord>[])
                .any((tab) => tab.id == tabId)) {
          throw const _McpUiFailure('tab_missing');
        }
        // This is view-only, even when the target is a Terminal Tab.
        await _mcpSelectAndVerifyTab(workspaceId, tabId);
        return tabId;
      case 'focus_agent':
        final handle = arguments['handle'];
        final tabId = arguments['tabId'];
        if (handle is! String || tabId is! String) {
          throw const _McpUiFailure('agent_missing');
        }
        final tabs =
            ref
                .read(workbenchControllerProvider)
                .tabsByWorkspace[workspaceId] ??
            const <WorkspaceTabRecord>[];
        final targets = tabs.where(
          (tab) =>
              tab.id == tabId &&
              tab.kind == WorkspaceTabKind.terminal &&
              tab.payload[workspaceTabTerminalSessionIdPayloadKey] == handle,
        );
        if (targets.length != 1) {
          throw const _McpUiFailure('agent_missing');
        }
        // This only selects the existing Tab; no PTY is created or restarted.
        await _mcpSelectAndVerifyTab(workspaceId, tabId);
        return tabId;
      case 'open_file':
        final position = _mcpFilePosition(arguments);
        final path = await _mcpExistingWorkspacePath(
          workspace,
          arguments['relativePath'],
        );
        final tab = await controller.openEditorTab(
          workspace: workspace,
          relativePath: path,
          forceNew: arguments['openMode'] == 'new',
        );
        await _mcpSelectAndVerifyTab(workspaceId, tab.id);
        if (position.line != null) {
          final navigated = await ref
              .read(editorSessionRegistryProvider)
              .revealAndWait(
                tab.id,
                WorkspaceEditorRevealTarget(
                  line: position.line!,
                  column: position.column ?? 1,
                  matchLength: 0,
                ),
              );
          if (!navigated || !mounted) {
            throw const _McpUiFailure('not_visible');
          }
          final active = ref.read(workbenchControllerProvider);
          if (active.activeWorkspaceId != workspaceId ||
              active.activeWorkspaceTab?.id != tab.id) {
            throw const _McpUiFailure('not_visible');
          }
        }
        return tab.id;
      case 'reveal_file':
        final path = await _mcpExistingWorkspacePath(
          workspace,
          arguments['relativePath'],
        );
        controller.revealInExplorer(workspace: workspace, relativePath: path);
        final request = ref.read(workspaceExplorerRevealControllerProvider);
        if (request == null ||
            request.workspaceId != workspaceId ||
            request.relativePath != path) {
          throw const _McpUiFailure('ui_not_ready');
        }
        final revealed = await ref
            .read(workspaceExplorerRevealControllerProvider.notifier)
            .waitForConsumed(request, timeout: const Duration(seconds: 8));
        if (!revealed ||
            !mounted ||
            ref.read(workbenchControllerProvider).activeWorkspaceId !=
                workspaceId) {
          throw const _McpUiFailure('not_visible');
        }
        return null;
      case 'open_git_graph':
        final scope = await _mcpGitScope(project, workspace, arguments);
        final file = arguments['filePath'] is String
            ? await _mcpExistingWorkspacePath(workspace, arguments['filePath'])
            : null;
        final tab = await controller.openGitHistoryTab(
          workspace: workspace,
          gitDiffRoot: scope.relativeRoot,
          filePath: file,
        );
        if (arguments.containsKey('allBranches') ||
            arguments.containsKey('favoritesOnly') ||
            arguments.containsKey('selectedRef')) {
          await controller.setGitHistoryAllBranches(
            tabId: tab.id,
            allBranches: arguments['allBranches'] == true,
            favoritesOnly: arguments['favoritesOnly'] == true,
            selectedRef: arguments['selectedRef'] as String?,
          );
        }
        await _mcpSelectAndVerifyTab(workspaceId, tab.id);
        return tab.id;
      case 'compare_branches':
        final scope = await _mcpGitScope(project, workspace, arguments);
        final baseRef = arguments['baseRef'];
        final headRef = arguments['headRef'];
        if (baseRef is! String || headRef is! String) {
          throw const _McpUiFailure('git_compare_failed');
        }
        final comparison = await ref
            .read(gitBackendProvider)
            .compareRange(path: scope.path, baseRef: baseRef, headRef: headRef);
        final summary = comparison.summary;
        final mergeBase = summary.parentOid;
        if (summary.status != GitCommitCompareStatus.ready ||
            mergeBase == null) {
          throw const _McpUiFailure('git_compare_failed');
        }
        final tab = await controller.openGitBranchCompareTab(
          workspace: workspace,
          gitDiffRoot: scope.relativeRoot,
          baseRef: baseRef,
          headRef: headRef,
          headOid: summary.commitOid,
          mergeBaseOid: mergeBase,
        );
        await _mcpSelectAndVerifyTab(workspaceId, tab.id);
        return tab.id;
      default:
        throw const _McpUiFailure('ui_operation_failed');
    }
  }

  Future<void> _mcpSelectAndVerifyTab(String workspaceId, String tabId) async {
    final controller = ref.read(workbenchControllerProvider.notifier);
    await controller.selectWorkspaceTab(workspaceId: workspaceId, tabId: tabId);
    if (!mounted) throw const _McpUiFailure('ui_not_ready');
    final current = ref.read(workbenchControllerProvider);
    if (current.activeWorkspaceId != workspaceId ||
        current.activeWorkspaceTab?.id != tabId) {
      throw const _McpUiFailure('not_visible');
    }
    await WidgetsBinding.instance.endOfFrame;
    if (!mounted) throw const _McpUiFailure('ui_not_ready');
    final visible = ref.read(workbenchControllerProvider);
    if (visible.activeWorkspaceId != workspaceId ||
        visible.activeWorkspaceTab?.id != tabId) {
      throw const _McpUiFailure('not_visible');
    }
  }

  ({int? line, int? column}) _mcpFilePosition(Map<String, Object?> arguments) {
    final line = arguments['line'];
    final column = arguments['column'];
    if ((line != null && (line is! int || line < 1 || line > 1000000000)) ||
        (column != null &&
            (column is! int || column < 1 || column > 1000000000)) ||
        (column != null && line == null)) {
      throw const _McpUiFailure('invalid_position');
    }
    return (line: line as int?, column: column as int?);
  }

  Future<void> _mcpOpenFileExternally(
    Workspace workspace,
    Map<String, Object?> arguments,
  ) async {
    final kind = switch (arguments['editor']) {
      'zed' => ExternalEditorKind.zed,
      'vscode' => ExternalEditorKind.vscode,
      _ => throw const _McpUiFailure('invalid_editor'),
    };
    if (arguments['openMode'] != null && arguments['openMode'] != 'reuse') {
      throw const _McpUiFailure('invalid_editor');
    }
    final position = _mcpFilePosition(arguments);
    final path = await _mcpExistingWorkspacePath(
      workspace,
      arguments['relativePath'],
    );
    final launcher = ref.read(externalEditorLauncherForProvider(kind));
    if (!await launcher.isInstalled()) {
      throw const _McpUiFailure('external_editor_unavailable');
    }
    final result = await launcher.openFile(
      ExternalEditorOpenRequest(
        workspacePath: workspace.path,
        filePath: p.joinAll(<String>[workspace.path, ...path.split('/')]),
        line: position.line,
        column: position.column,
      ),
    );
    if (!result.ok) {
      throw _McpUiFailure(
        result.failureKind == ExternalEditorLaunchFailureKind.unavailable
            ? 'external_editor_unavailable'
            : 'external_editor_failed',
      );
    }
  }

  Future<String> _mcpExistingWorkspacePath(
    Workspace workspace,
    Object? rawPath,
  ) => ref.read(mcpWorkspaceFilePathResolverProvider)(workspace, rawPath);

  Future<WorkspaceSourceControlScope> _mcpGitScope(
    Project project,
    Workspace workspace,
    Map<String, Object?> arguments,
  ) async {
    final explicitRoot = arguments['gitDiffRoot'];
    if (explicitRoot != null) {
      final normalized = explicitRoot is String
          ? normalizeWorkspaceRelativePath(explicitRoot)
          : null;
      if (normalized == null ||
          normalized.contains(':') ||
          normalized.startsWith('/')) {
        throw const _McpUiFailure('invalid_path');
      }
      final nested = Directory(
        p.joinAll(<String>[workspace.path, ...normalized.split('/')]),
      );
      if (!await nested.exists()) {
        throw const _McpUiFailure('invalid_path');
      }
      final root = await Directory(workspace.path).resolveSymbolicLinks();
      if (!p.isWithin(root, await nested.resolveSymbolicLinks())) {
        throw const _McpUiFailure('invalid_path');
      }
      return WorkspaceSourceControlScope(
        workspaceId: workspace.id,
        workspacePath: workspace.path,
        path: nested.path,
        relativeRoot: normalized,
      );
    }
    final state = ref.read(workbenchControllerProvider);
    final scope = WorkspaceSourceControlScope.resolve(
      project: project,
      workspace: workspace,
      prefs: state.viewPrefs,
    );
    if (scope == null) throw const _McpUiFailure('git_compare_failed');
    return scope;
  }
}

final class _McpUiFailure implements Exception {
  const _McpUiFailure(this.code);
  final String code;
}
