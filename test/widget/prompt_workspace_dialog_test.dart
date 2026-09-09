import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_profile.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/infra/prompt_workspace_clipboard.dart';
import 'package:alera/src/features/workbench/infra/prompt_workspace_runtime_client.dart';
import 'package:alera/src/features/workbench/presentation/prompt_workspace_dialog.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

part 'prompt_workspace_dialog_clipboard_test_cases.dart';
part 'prompt_workspace_dialog_test_support.dart';

void main() {
  _registerPromptWorkspaceClipboardTests();

  testWidgets('creates an AI-named workspace and launches the profile', (
    tester,
  ) async {
    final now = DateTime.utc(2026, 7, 29);
    final project = _project(id: 'project-1', name: 'Alera', now: now);
    final profile = _profile(id: 'profile-1', name: 'Codex Builder', now: now);
    final preferredProfile = _profile(
      id: 'profile-2',
      name: 'Claude Reviewer',
      agentType: 'claude',
      command: 'claude',
      now: now,
    );
    PromptWorkspaceDialogResult? dialogResult;
    String? generatedPrompt;
    String? createdBranch;
    String? createdName;
    String? createdParentWorkspaceId;
    String? launchedPrompt;
    String? launchedProfileId;

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          settingsControllerProvider.overrideWith(
            _PromptSettingsController.new,
          ),
        ],
        child: MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: FilledButton(
                onPressed: () async {
                  dialogResult = await showDialog<PromptWorkspaceDialogResult>(
                    context: context,
                    builder: (_) => PromptWorkspaceDialog(
                      projects: <Project>[project],
                      agentProfiles: <AgentProfile>[profile, preferredProfile],
                      defaultAgentProfileId: preferredProfile.id,
                      loadBranches: (_) async => <String>['main'],
                      checkBranchExists: (_, _) async => false,
                      workspaceBranches: (_) => const <String>{},
                      parentWorkspaces: const <Workspace>[],
                      generateIdentity:
                          ({
                            required operationId,
                            required projectId,
                            required prompt,
                          }) async {
                            generatedPrompt = prompt;
                            return const GeneratedWorkspaceIdentity(
                              workspaceName: 'Prompt Workspace',
                              branchName: 'feat/prompt-workspace',
                            );
                          },
                      cancelGeneration: (_) async {},
                      createWorkspace:
                          ({
                            required project,
                            required sourceBranch,
                            required newBranchName,
                            required name,
                            parentWorkspaceId,
                          }) async {
                            createdBranch = newBranchName;
                            createdName = name;
                            createdParentWorkspaceId = parentWorkspaceId;
                            return WorkspaceCreationResult(
                              workspace: Workspace(
                                id: 'workspace-1',
                                projectId: project.id,
                                name: name,
                                branch: newBranchName,
                                sourceBranch: sourceBranch,
                                path: '/repo/alera-workspace',
                                kind: .linked,
                                status: .active,
                                createdAt: now,
                                updatedAt: now,
                              ),
                              setupReport: .empty,
                            );
                          },
                      launchAgent:
                          ({
                            required workspaceId,
                            required profileId,
                            required prompt,
                            required clientMutationId,
                            required requireIdempotency,
                          }) async {
                            launchedPrompt = prompt;
                            launchedProfileId = profileId;
                            return AgentProfileLaunchResult(
                              tabId: 'tab-1',
                              agentType: preferredProfile.agentType,
                              profileId: profileId,
                              idempotent: true,
                            );
                          },
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
    final promptField = tester.getRect(
      find.widgetWithText(TextField, 'Initial Prompt'),
    );
    final dictationControl = tester.getRect(
      find.byKey(const ValueKey<String>('prompt-workspace-dictation-control')),
    );
    expect(
      dictationControl.top - promptField.top,
      lessThanOrEqualTo(AleraTokens.space12),
    );
    expect(
      promptField.right - dictationControl.right,
      lessThanOrEqualTo(AleraTokens.space12),
    );
    await tester.enterText(
      find.widgetWithText(TextField, 'Initial Prompt'),
      'Build workspace creation',
    );
    final submit = find.text('Create And Start Agent');
    await tester.ensureVisible(submit);
    await tester.tap(submit);
    await tester.pumpAndSettle();

    expect(generatedPrompt, 'Build workspace creation');
    expect(createdBranch, 'feat/prompt-workspace');
    expect(createdName, 'Prompt Workspace');
    expect(createdParentWorkspaceId, isNull);
    expect(launchedPrompt, 'Build workspace creation');
    expect(launchedProfileId, preferredProfile.id);
    expect(dialogResult?.creation?.workspace.id, 'workspace-1');
    expect(dialogResult?.agentTabId, 'tab-1');
  });

  testWidgets('Create Another preserves selections', (tester) async {
    final now = DateTime.utc(2026, 7, 30);
    final project = _project(id: 'project-1', name: 'Alera', now: now);
    final profile = _profile(id: 'profile-1', name: 'Codex Builder', now: now);
    final alternateProfile = _profile(
      id: 'profile-2',
      name: 'Codex Reviewer',
      now: now,
    );
    final featureWorkspace = _workspace(
      id: 'workspace-feature',
      projectId: project.id,
      name: 'Feature Workspace',
      branch: 'feat/parent',
      kind: .linked,
      now: now,
    );
    var createAnotherCallbacks = 0;

    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: FilledButton(
              onPressed: () {
                showDialog<PromptWorkspaceDialogResult>(
                  context: context,
                  builder: (_) => PromptWorkspaceDialog(
                    projects: <Project>[project],
                    agentProfiles: <AgentProfile>[profile, alternateProfile],
                    loadBranches: (_) async => <String>['main', 'release'],
                    checkBranchExists: (_, _) async => false,
                    workspaceBranches: (_) => const <String>{},
                    parentWorkspaces: <Workspace>[featureWorkspace],
                    generateIdentity:
                        ({
                          required operationId,
                          required projectId,
                          required prompt,
                        }) async => const GeneratedWorkspaceIdentity(
                          workspaceName: 'Prompt Workspace',
                          branchName: 'feat/prompt-workspace',
                        ),
                    cancelGeneration: (_) async {},
                    createWorkspace:
                        ({
                          required project,
                          required sourceBranch,
                          required newBranchName,
                          required name,
                          parentWorkspaceId,
                        }) async {
                          return WorkspaceCreationResult(
                            workspace: Workspace(
                              id: 'workspace-1',
                              projectId: project.id,
                              name: name,
                              branch: newBranchName,
                              sourceBranch: sourceBranch,
                              path: '/repo/alera-workspace',
                              kind: .linked,
                              status: .active,
                              createdAt: now,
                              updatedAt: now,
                            ),
                            setupReport: .empty,
                          );
                        },
                    launchAgent:
                        ({
                          required workspaceId,
                          required profileId,
                          required prompt,
                          required clientMutationId,
                          required requireIdempotency,
                        }) async => const AgentProfileLaunchResult(
                          tabId: 'tab-1',
                          agentType: 'codex',
                          profileId: 'profile-1',
                          idempotent: true,
                        ),
                    supportsIdempotentAgentLaunch: () async => true,
                    onCreateAnother:
                        ({required creation, required agentTabId}) async {
                          createAnotherCallbacks += 1;
                        },
                  ),
                );
              },
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    AleraDropdownField<T> field<T>(String label) {
      return tester.widget<AleraDropdownField<T>>(
        find.byWidgetPredicate(
          (widget) =>
              widget is AleraDropdownField<T> && widget.labelText == label,
        ),
      );
    }

    field<String>('Source Branch').onChanged('release');
    field<String?>('Parent Workspace').onChanged(featureWorkspace.id);
    field<AgentProfile>('Agent Profile').onChanged(alternateProfile);
    await tester.pump();

    await tester.enterText(
      find.widgetWithText(TextField, 'Initial Prompt'),
      'Build the first workspace',
    );
    await tester.ensureVisible(find.text('Create Another'));
    await tester.tap(find.text('Create Another'));
    await tester.pump();
    await tester.drag(
      find.byType(SingleChildScrollView),
      const Offset(0, -120),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Create And Start Agent'));
    await tester.pumpAndSettle();

    expect(createAnotherCallbacks, 1);
    expect(find.text('New Workspace'), findsOneWidget);
    expect(
      tester
          .widget<TextField>(find.widgetWithText(TextField, 'Initial Prompt'))
          .controller
          ?.text,
      isEmpty,
    );
    expect(find.text('Create Another'), findsOneWidget);
    expect(field<Project>('Project').value, project);
    expect(field<String>('Source Branch').value, 'release');
    expect(field<String?>('Parent Workspace').value, featureWorkspace.id);
    expect(field<AgentProfile>('Agent Profile').value, alternateProfile);
  });

  testWidgets(
    'defaults the parent to the selected project main workspace and allows changing it',
    (tester) async {
      final now = DateTime.utc(2026, 7, 30);
      final alera = _project(id: 'project-alera', name: 'Alera', now: now);
      final orca = _project(id: 'project-orca', name: 'Orca', now: now);
      final profile = _profile(
        id: 'profile-1',
        name: 'Codex Builder',
        now: now,
      );
      final aleraMain = _workspace(
        id: 'alera-main',
        projectId: alera.id,
        name: 'Alera',
        branch: 'main',
        kind: .main,
        now: now,
      );
      final orcaMain = _workspace(
        id: 'orca-main',
        projectId: orca.id,
        name: 'Orca',
        branch: 'main',
        kind: .main,
        now: now,
      );
      final orcaFeature = _workspace(
        id: 'orca-feature',
        projectId: orca.id,
        name: 'Feature Workspace',
        branch: 'feat/other',
        kind: .linked,
        now: now,
      );
      String? createdParentWorkspaceId;

      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: FilledButton(
                onPressed: () {
                  showDialog<PromptWorkspaceDialogResult>(
                    context: context,
                    builder: (_) => PromptWorkspaceDialog(
                      projects: <Project>[orca, alera],
                      agentProfiles: <AgentProfile>[profile],
                      loadBranches: (_) async => <String>['main'],
                      checkBranchExists: (_, _) async => false,
                      workspaceBranches: (_) => const <String>{},
                      parentWorkspaces: <Workspace>[
                        aleraMain,
                        orcaMain,
                        orcaFeature,
                      ],
                      generateIdentity:
                          ({
                            required operationId,
                            required projectId,
                            required prompt,
                          }) async => const GeneratedWorkspaceIdentity(
                            workspaceName: 'Prompt Workspace',
                            branchName: 'feat/prompt-workspace',
                          ),
                      cancelGeneration: (_) async {},
                      createWorkspace:
                          ({
                            required project,
                            required sourceBranch,
                            required newBranchName,
                            required name,
                            parentWorkspaceId,
                          }) async {
                            createdParentWorkspaceId = parentWorkspaceId;
                            return WorkspaceCreationResult(
                              workspace: _workspace(
                                id: 'created',
                                projectId: project.id,
                                name: name,
                                branch: newBranchName,
                                kind: .linked,
                                now: now,
                              ),
                              setupReport: .empty,
                            );
                          },
                      launchAgent:
                          ({
                            required workspaceId,
                            required profileId,
                            required prompt,
                            required clientMutationId,
                            required requireIdempotency,
                          }) async => const AgentProfileLaunchResult(
                            tabId: 'tab-1',
                            agentType: 'codex',
                            profileId: 'profile-1',
                            idempotent: true,
                          ),
                      supportsIdempotentAgentLaunch: () async => true,
                    ),
                  );
                },
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      AleraDropdownField<String?> parentField() {
        return tester.widget<AleraDropdownField<String?>>(
          find.byWidgetPredicate(
            (widget) =>
                widget is AleraDropdownField<String?> &&
                widget.labelText == 'Parent Workspace',
          ),
        );
      }

      expect(parentField().value, aleraMain.id);
      final projectField = tester.widget<AleraDropdownField<Project>>(
        find.byWidgetPredicate(
          (widget) =>
              widget is AleraDropdownField<Project> &&
              widget.labelText == 'Project',
        ),
      );
      expect(projectField.entries.map((entry) => entry.value.id), <String>[
        alera.id,
        orca.id,
      ]);
      projectField.onChanged(orca);
      await tester.pumpAndSettle();

      expect(parentField().value, orcaMain.id);
      expect(parentField().entries.map((entry) => entry.value), <String?>[
        null,
        orcaMain.id,
        orcaFeature.id,
        aleraMain.id,
      ]);

      parentField().onChanged(orcaFeature.id);
      await tester.pump();
      expect(parentField().value, orcaFeature.id);

      parentField().onChanged(null);
      await tester.pump();
      expect(parentField().value, isNull);

      parentField().onChanged(orcaFeature.id);
      await tester.pump();
      await tester.enterText(
        find.widgetWithText(TextField, 'Initial Prompt'),
        'Build another workspace',
      );
      final submit = find.text('Create And Start Agent');
      await tester.ensureVisible(submit);
      await tester.tap(submit);
      await tester.pumpAndSettle();

      expect(createdParentWorkspaceId, orcaFeature.id);
    },
  );

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
