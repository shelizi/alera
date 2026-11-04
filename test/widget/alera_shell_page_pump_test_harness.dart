part of 'alera_shell_page_test.dart';

Future<AleraDatabase> _openMemoryDb() async {
  return AleraDatabase(executor: NativeDatabase.memory());
}

Future<_ShellPumpHarness> _pumpShell(
  WidgetTester tester, {
  required WorkbenchState state,
  _FakeTerminalRuntime? terminalRuntime,
  WorkspaceFolderOpener? workspaceFolderOpener,
  _ShellTestWorkbenchController? controller,
  EditorSessionRegistry? editorSessionRegistry,
  RuntimeHostClient? mcpRuntimeHostClient,
  McpWorkspaceFilePathResolver? mcpFilePathResolver,
  Map<ExternalEditorKind, ExternalEditorLauncher> mcpExternalEditorLaunchers =
      const <ExternalEditorKind, ExternalEditorLauncher>{},
  AleraSettings? settings,
  Map<String, AgentStatusEntry> agentStatuses =
      const <String, AgentStatusEntry>{},
  bool agentTitlesAvailable = false,
  bool terminalTransferAvailable = false,
  List<AgentType> installedAgents = const <AgentType>[],
  ExternalTerminalLauncher? externalTerminalLauncher,
  VoidCallback? onAgentHookInstallerCoordinatorStarted,
}) async {
  final shellController = controller ?? _ShellTestWorkbenchController(state);
  final runtime = terminalRuntime ?? _FakeTerminalRuntime();
  final terminalHostClient = FakeTerminalHostClient(
    attachment: TerminalHostAttachment(
      sessionId: 'shell-test-session',
      created: true,
      running: true,
      snapshot: Uint8List(0),
    ),
  );
  addTearDown(terminalHostClient.dispose);
  final settingsController = _ShellSettingsController(
    settings ?? AleraSettings.defaults,
  );
  final agentStatusController = _ShellTestAgentStatusController(agentStatuses);
  final db = await _openMemoryDb();
  addTearDown(db.close);

  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        aleraDatabaseProvider.overrideWith((ref) async => db),
        workbenchControllerProvider.overrideWith(() => shellController),
        workbenchTerminalTransferAvailabilityProvider.overrideWith(
          (ref) async => terminalTransferAvailable,
        ),
        agentProfilesProvider.overrideWith(() => _ShellAgentProfiles()),
        agentStatusControllerProvider.overrideWith(() => agentStatusController),
        agentQuotaStateProvider.overrideWith(
          () => _ShellAgentQuotaSnapshotList(
            state.activeWorkspace?.hostId ?? 'local',
          ),
        ),
        managedWorkspaceRuntimeProvider.overrideWithValue(
          const _FakeManagedWorkspaceRuntime(),
        ),
        terminalRuntimeProvider.overrideWith((ref) => runtime),
        terminalHostClientProvider.overrideWithValue(terminalHostClient),
        if (mcpRuntimeHostClient != null)
          runtimeHostClientProvider.overrideWithValue(mcpRuntimeHostClient),
        if (mcpRuntimeHostClient != null)
          terminalSessionTitleSourceProvider.overrideWithValue(
            const _McpNavigationTitleSource(),
          ),
        if (mcpFilePathResolver != null)
          mcpWorkspaceFilePathResolverProvider.overrideWithValue(
            mcpFilePathResolver,
          ),
        for (final entry in mcpExternalEditorLaunchers.entries)
          externalEditorLauncherForProvider(entry.key)
              .overrideWithValue(entry.value),
        terminalRuntimeBindingOverride(),
        if (editorSessionRegistry != null)
          editorSessionRegistryProvider.overrideWithValue(
            editorSessionRegistry,
          ),
        terminalHostWarmupCoordinatorProvider.overrideWith((ref) {}),
        if (onAgentHookInstallerCoordinatorStarted != null)
          agentHookInstallerCoordinatorProvider.overrideWith((ref) {
            onAgentHookInstallerCoordinatorStarted();
          }),
        settingsControllerProvider.overrideWith(() => settingsController),
        agentTitleAvailableProvider.overrideWith(
          (ref) async => agentTitlesAvailable,
        ),
        installedAgentClisProvider.overrideWith((ref) async => installedAgents),
        if (externalTerminalLauncher != null)
          externalTerminalLauncherProvider.overrideWith(
            (ref) => externalTerminalLauncher,
          ),
        installedExternalEditorsProvider.overrideWith(
          (ref) async => const <ExternalEditorSpec>[],
        ),
        resolvedExternalEditorProvider.overrideWith((ref) async => null),
        if (workspaceFolderOpener != null)
          workspaceFolderOpenerProvider.overrideWith(
            (ref) => workspaceFolderOpener,
          ),
      ],
      child: MaterialApp(
        home: AleraShellPage(key: const ValueKey<String>('alera-shell-page')),
      ),
    ),
  );
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 200));
  return _ShellPumpHarness(
    controller: shellController,
    runtime: runtime,
    agentStatus: agentStatusController,
  );
}

class _ShellAgentProfiles extends AgentProfiles {
  @override
  Future<List<AgentProfile>> build() async => const <AgentProfile>[];
}

class _ShellAgentQuotaSnapshotList extends AgentQuotaSnapshotList {
  _ShellAgentQuotaSnapshotList(this.hostId);

  final String hostId;

  @override
  Future<AgentQuotaState> build() async => AgentQuotaState.empty(hostId);
}
