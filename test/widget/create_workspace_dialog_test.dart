import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:alera/src/features/workbench/presentation/create_workspace_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

part 'create_workspace_dialog_branch_test_cases.dart';

typedef MockSubmitResult = ({
  Project project,
  String sourceBranch,
  String newBranchName,
  bool reuseExistingBranch,
  String? name,
  String? parentWorkspaceId,
});

void main() {
  _registerCreateWorkspaceDialogBranchTests();
  testWidgets('selects a project, filters source branches, and submits', (
    tester,
  ) async {
    MockSubmitResult? result;
    final projects = <Project>[_project(id: 'alera', name: 'Alera'), _orca()];

    await _pumpDialogLauncher(
      tester,
      projects: projects,
      loadBranches: (project) async {
        if (project.id == 'orca') {
          return const <String>['develop', 'feature/orchestration'];
        }
        return const <String>['main', 'origin/main'];
      },
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'Search projects'),
      'orca',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Orca'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'Search source branches'),
      'feature',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('feature/orchestration'));
    await tester.pumpAndSettle();

    // Tap Continue to go to Step 2
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'New Branch Name *'),
      'feature/workspace-imports',
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'Workspace Name (Optional)'),
      'Workspace imports',
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Create Workspace'));
    await tester.pumpAndSettle();

    expect(result, isNotNull);
    expect(result!.project.id, 'orca');
    expect(result!.sourceBranch, 'feature/orchestration');
    expect(result!.newBranchName, 'feature/workspace-imports');
    expect(result!.reuseExistingBranch, isFalse);
    expect(result!.name, 'Workspace imports');
  });

  testWidgets('preselects the requested project and default branch', (
    tester,
  ) async {
    MockSubmitResult? result;
    final projects = <Project>[_project(id: 'alera', name: 'Alera'), _orca()];

    await _pumpDialogLauncher(
      tester,
      projects: projects,
      initialProject: _orca(),
      loadBranches: (project) async {
        expect(project.id, 'orca');
        return const <String>['develop', 'main'];
      },
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    // Tap Continue to go to Step 2
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'New Branch Name *'),
      'feature/default-source',
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Create Workspace'));
    await tester.pumpAndSettle();

    expect(result, isNotNull);
    expect(result!.project.id, 'orca');
    expect(result!.sourceBranch, 'main');
  });

  testWidgets('changing projects reloads source branches', (tester) async {
    MockSubmitResult? result;
    final loadedProjectIds = <String>[];
    final projects = <Project>[_project(id: 'alera', name: 'Alera'), _orca()];

    await _pumpDialogLauncher(
      tester,
      projects: projects,
      loadBranches: (project) async {
        loadedProjectIds.add(project.id);
        if (project.id == 'orca') {
          return const <String>['release/orca'];
        }
        return const <String>['main'];
      },
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Orca'));
    await tester.pumpAndSettle();

    // Tap Continue to go to Step 2
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'New Branch Name *'),
      'release/orca-workspace',
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Create Workspace'));
    await tester.pumpAndSettle();

    expect(loadedProjectIds, <String>['alera', 'orca']);
    expect(result, isNotNull);
    expect(result!.sourceBranch, 'release/orca');
  });

  testWidgets(
    'requires source branch and new branch when no branch list is provided',
    (tester) async {
      await _pumpDialogLauncher(
        tester,
        projects: <Project>[_project()],
        loadBranches: (_) async => const <String>[],
        onSubmit: (_) {},
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      // Try to continue without source branch
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      expect(find.text('Source branch is required'), findsOneWidget);

      // Input source branch and continue
      await tester.enterText(
        find.widgetWithText(TextField, 'Source Branch'),
        'main',
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      // Try to create workspace without new branch name
      await tester.tap(find.text('Create Workspace'));
      await tester.pumpAndSettle();
      expect(find.text('New branch name is required'), findsOneWidget);
    },
  );

  testWidgets('shows empty states for project and branch filters', (
    tester,
  ) async {
    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>['main'],
      onSubmit: (_) {},
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Search projects'),
      'missing',
    );
    await tester.pumpAndSettle();
    expect(find.text('No projects match "missing"'), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextField, 'Search projects'),
      '',
    );
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Search source branches'),
      'missing',
    );
    await tester.pumpAndSettle();
    expect(find.text('No source branches match "missing"'), findsOneWidget);
  });

  testWidgets('keeps branch picker scrollable on compact desktop heights', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1224, 768));
    addTearDown(() => tester.binding.setSurfaceSize(null));

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>[
        'dev',
        'main',
        'origin',
        'origin/claude/integrate-vercel-analytics-01KxPescNmVV1Rr4T2RVutVg',
        'origin/dev',
        'origin/main',
        'origin/v0/leynier-24f7f479',
        'origin/v0/leynier-aac72b82',
      ],
      onSubmit: (_) {},
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
    expect(find.text('Continue'), findsOneWidget);
  });

  testWidgets('shows branch load errors and can cancel the dialog', (
    tester,
  ) async {
    MockSubmitResult? result;

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => throw StateError('cannot load branches'),
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    expect(find.text('Bad state: cannot load branches'), findsOneWidget);

    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();

    expect(result, isNull);
  });

  testWidgets(
    'filters parent candidates to the selected project and clears a stale selection',
    (tester) async {
      final alera = _project(id: 'alera', name: 'Alera');
      final orca = _orca();
      final aleraMain = _workspace(
        id: 'alera-main',
        projectId: alera.id,
        name: 'Alera',
      );
      final orcaMain = _workspace(
        id: 'orca-main',
        projectId: orca.id,
        name: 'Orca',
      );

      await _pumpDialogLauncher(
        tester,
        projects: <Project>[alera, orca],
        parentCandidates: <WorkspaceParentCandidate>[
          WorkspaceParentCandidate(project: alera, workspace: aleraMain),
          WorkspaceParentCandidate(project: orca, workspace: orcaMain),
        ],
        loadBranches: (_) async => const <String>['main'],
        onSubmit: (_) {},
      );

      AleraDropdownField<String?> parentField() {
        return tester.widget<AleraDropdownField<String?>>(
          find.byWidgetPredicate(
            (widget) =>
                widget is AleraDropdownField<String?> &&
                widget.labelText == 'Parent Workspace',
          ),
        );
      }

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Orca'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      expect(parentField().entries.map((entry) => entry.value), <String?>[
        null,
        'orca-main',
      ]);

      parentField().onChanged('orca-main');
      await tester.pump();
      expect(parentField().value, 'orca-main');

      await tester.tap(find.text('Back'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Alera'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      expect(parentField().value, isNull);
      expect(parentField().entries.map((entry) => entry.value), <String?>[
        null,
        'alera-main',
      ]);
    },
  );
}

Future<void> _pumpDialogLauncher(
  WidgetTester tester, {
  required List<Project> projects,
  required Future<List<String>> Function(Project project) loadBranches,
  required ValueChanged<MockSubmitResult?> onSubmit,
  Project? initialProject,
  List<WorkspaceParentCandidate> parentCandidates =
      const <WorkspaceParentCandidate>[],
  Set<String> existingBranches = const <String>{},
  Future<bool> Function(Project project, String branch)? checkBranchExists,
  String? Function(Project project)? getProjectActiveBranch,
  Set<String> Function(Project project)? getProjectWorkspaceBranches,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Builder(
        builder: (context) {
          return Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () async {
                  await showDialog<WorkspaceCreationResult>(
                    context: context,
                    builder: (_) => CreateWorkspaceDialog(
                      projects: projects,
                      initialProject: initialProject,
                      parentCandidates: parentCandidates,
                      loadBranches: loadBranches,
                      getProjectActiveBranch:
                          getProjectActiveBranch ?? ((_) => null),
                      getProjectWorkspaceBranches:
                          getProjectWorkspaceBranches ??
                          ((project) {
                            final activeBranch = getProjectActiveBranch?.call(
                              project,
                            );
                            return activeBranch == null
                                ? const <String>{}
                                : <String>{activeBranch};
                          }),
                      checkBranchExists:
                          checkBranchExists ??
                          (_, branch) async =>
                              existingBranches.contains(branch),
                      onCreateWorkspace:
                          ({
                            required project,
                            required sourceBranch,
                            required newBranchName,
                            required reuseExistingBranch,
                            name,
                            parentWorkspaceId,
                          }) async {
                            onSubmit((
                              project: project,
                              sourceBranch: sourceBranch,
                              newBranchName: newBranchName,
                              reuseExistingBranch: reuseExistingBranch,
                              name: name,
                              parentWorkspaceId: parentWorkspaceId,
                            ));
                            return WorkspaceCreationResult(
                              workspace: Workspace(
                                id: 'workspace-1',
                                projectId: project.id,
                                name: name ?? newBranchName,
                                branch: newBranchName,
                                sourceBranch: sourceBranch,
                                path: project.repoPath,
                                createdAt: .utc(2026, 6, 27),
                                updatedAt: .utc(2026, 6, 27),
                                kind: .linked,
                                status: .active,
                              ),
                              setupReport: .empty,
                            );
                          },
                    ),
                  );
                },
                child: const Text('Open'),
              ),
            ),
          );
        },
      ),
    ),
  );
}

Project _project({String id = 'project-1', String name = 'Alera'}) {
  final now = DateTime.utc(2026, 5, 21);
  return Project(
    id: id,
    name: name,
    repoPath: '/repo/$id',
    createdAt: now,
    updatedAt: now,
  );
}

Project _orca() => _project(id: 'orca', name: 'Orca');

Workspace _workspace({
  required String id,
  required String projectId,
  required String name,
}) {
  final now = DateTime.utc(2026, 5, 21);
  return Workspace(
    id: id,
    projectId: projectId,
    name: name,
    branch: 'main',
    path: '/repo/$projectId/$id',
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}
