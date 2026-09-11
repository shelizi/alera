part of 'prompt_workspace_dialog_test.dart';

void _registerPromptWorkspaceLocalizationTests() {
  testWidgets(
    'localizes prompt workspace chrome while preserving dynamic names',
    (tester) async {
      final now = DateTime.utc(2026, 9, 9);
      final project = _project(
        id: 'project-settings',
        name: 'Settings',
        now: now,
      );
      final profile = _profile(id: 'profile-editor', name: 'Editor', now: now);
      final parent = _workspace(
        id: 'workspace-project',
        projectId: project.id,
        name: 'Project',
        branch: 'Settings',
        kind: .main,
        now: now,
      );

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            settingsControllerProvider.overrideWith(
              _PromptSettingsController.new,
            ),
          ],
          child: MaterialApp(
            locale: const Locale('zh', 'TW'),
            supportedLocales: supportedAleraLocales,
            localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
              AleraLocalizationsDelegate(),
              ...GlobalMaterialLocalizations.delegates,
            ],
            home: Builder(
              builder: (context) => Scaffold(
                body: FilledButton(
                  onPressed: () {
                    showDialog<PromptWorkspaceDialogResult>(
                      context: context,
                      builder: (_) => PromptWorkspaceDialog(
                        projects: <Project>[project],
                        agentProfiles: <AgentProfile>[profile],
                        loadBranches: (_) async => <String>['Editor'],
                        checkBranchExists: (_, _) async => false,
                        workspaceBranches: (_) => const <String>{},
                        parentWorkspaces: <Workspace>[parent],
                        generateIdentity:
                            ({
                              required operationId,
                              required projectId,
                              required prompt,
                            }) async => const GeneratedWorkspaceIdentity(
                              workspaceName: 'Generated Workspace',
                              branchName: 'generated/branch',
                            ),
                        cancelGeneration: (_) async {},
                        createWorkspace: ({
                          required project,
                          required sourceBranch,
                          required newBranchName,
                          required name,
                          parentWorkspaceId,
                        }) async => throw UnimplementedError(),
                        launchAgent: ({
                          required workspaceId,
                          required profileId,
                          required prompt,
                          required clientMutationId,
                          required requireIdempotency,
                        }) async => throw UnimplementedError(),
                        supportsIdempotentAgentLaunch: () async => true,
                      ),
                    );
                  },
                  child: const Text('Open'),
                ),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      expect(find.text('新增工作區'), findsOneWidget);
      expect(find.byTooltip('關閉'), findsOneWidget);
      expect(find.text('初始提示詞'), findsOneWidget);
      expect(find.text('專案'), findsOneWidget);
      expect(find.text('來源 Branch'), findsOneWidget);
      expect(find.text('父工作區'), findsOneWidget);
      expect(find.text('Agent 設定檔'), findsOneWidget);
      expect(find.text('繼續建立下一個'), findsOneWidget);

      expect(find.text('Settings'), findsOneWidget);
      expect(find.text('Editor'), findsNWidgets(2));
      expect(find.text('Settings / Project - Settings'), findsOneWidget);
      expect(find.text('設定'), findsNothing);
      expect(find.text('編輯器'), findsNothing);

      AleraDropdownField<T> field<T>(String label) {
        return tester.widget<AleraDropdownField<T>>(
          find.byWidgetPredicate(
            (widget) =>
                widget is AleraDropdownField<T> && widget.labelText == label,
          ),
        );
      }

      expect(
        field<Project>('Project').entries
            .every((entry) => !entry.localizeLabel),
        isTrue,
      );
      expect(
        field<String>('Source Branch').entries
            .every((entry) => !entry.localizeLabel),
        isTrue,
      );
      expect(
        field<String?>('Parent Workspace').entries
            .where((entry) => entry.value != null)
            .every((entry) => !entry.localizeLabel),
        isTrue,
      );
      expect(
        field<AgentProfile>('Agent Profile').entries
            .every((entry) => !entry.localizeLabel),
        isTrue,
      );

      final submit = find.text('建立並啟動 Agent');
      await tester.ensureVisible(submit);
      await tester.tap(submit);
      await tester.pump();
      expect(find.text('請完成提示詞、專案、Branch 與 Agent 設定檔。'), findsOneWidget);

      await tester.tap(find.text('手動'));
      await tester.pump();
      expect(find.text('自行選擇所有工作區設定，包括 Branch 名稱與選填的父工作區。'), findsOneWidget);
      expect(find.text('手動繼續'), findsOneWidget);
    },
  );
}
