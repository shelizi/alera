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
          .catchError((Object _, StackTrace __) {})
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
    String? errorCode;
    try {
      tabId = await _applyMcpUiCommand(command);
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
          if (errorCode != null) 'errorCode': errorCode,
        },
      );
    } catch (_) {
      // A disconnected Host or expired request must not be reported as
      // complete by the UI. The MCP caller will receive its Host-side error.
      Logger('AleraMcpUi').fine('MCP UI command receipt could not be sent.');
    }
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
    final matchingProject = before.projects.where(
      (item) => item.id == workspace!.projectId,
    );
    if (matchingProject.isEmpty) {
      throw const _McpUiFailure('workspace_missing');
    }
    final project = matchingProject.first;
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
        final path = await _mcpExistingWorkspacePath(
          workspace,
          arguments['relativePath'],
        );
        final tab = await controller.openEditorTab(
          workspace: workspace,
          relativePath: path,
        );
        await _mcpSelectAndVerifyTab(workspaceId, tab.id);
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

  Future<String> _mcpExistingWorkspacePath(
    Workspace workspace,
    Object? rawPath,
  ) async {
    final candidate = rawPath is String
        ? normalizeWorkspaceRelativePath(rawPath)
        : null;
    if (candidate == null ||
        candidate.startsWith('/') ||
        candidate.contains(':')) {
      throw const _McpUiFailure('invalid_path');
    }
    final file = File(
      p.joinAll(<String>[workspace.path, ...candidate.split('/')]),
    );
    if (!await file.exists()) {
      throw const _McpUiFailure('file_missing');
    }
    final root = await Directory(workspace.path).resolveSymbolicLinks();
    final resolved = await file.resolveSymbolicLinks();
    if (!p.isWithin(root, resolved)) {
      throw const _McpUiFailure('invalid_path');
    }
    return candidate;
  }

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
