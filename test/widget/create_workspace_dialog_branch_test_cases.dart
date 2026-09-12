part of 'create_workspace_dialog_test.dart';

void _registerCreateWorkspaceDialogBranchTests() {
  testWidgets('can create a workspace from an existing branch', (tester) async {
    MockSubmitResult? result;

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>[
        'main',
        'origin/main',
        'feature/reuse-me',
      ],
      existingBranches: const <String>{'main', 'feature/reuse-me'},
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Existing Branch'));
    await tester.pumpAndSettle();
    expect(find.text('origin/main (default)'), findsNothing);
    await tester.tap(find.text('feature/reuse-me'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    expect(find.widgetWithText(TextField, 'Existing Branch *'), findsOneWidget);

    await tester.enterText(
      find.widgetWithText(TextField, 'Workspace Name (Optional)'),
      'Reuse me',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Create Workspace'));
    await tester.pumpAndSettle();

    expect(result, isNotNull);
    expect(result!.sourceBranch, 'feature/reuse-me');
    expect(result!.newBranchName, 'feature/reuse-me');
    expect(result!.reuseExistingBranch, isTrue);
    expect(result!.name, 'Reuse me');
  });

  testWidgets('does not default existing branch to active project branch', (
    tester,
  ) async {
    MockSubmitResult? result;

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>[
        'main',
        'develop',
        'feature/reuse-me',
      ],
      existingBranches: const <String>{'main', 'develop', 'feature/reuse-me'},
      getProjectActiveBranch: (_) => 'main',
      getProjectWorkspaceBranches: (_) => const <String>{'main', 'develop'},
      onSubmit: (val) => result = val,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Existing Branch'));
    await tester.pumpAndSettle();

    expect(find.text('feature/reuse-me'), findsOneWidget);
    expect(find.text('main (default)'), findsNothing);
    expect(find.text('develop'), findsNothing);

    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Create Workspace'));
    await tester.pumpAndSettle();

    expect(result, isNotNull);
    expect(result!.sourceBranch, 'feature/reuse-me');
    expect(result!.newBranchName, 'feature/reuse-me');
    expect(result!.reuseExistingBranch, isTrue);
  });

  testWidgets('defers local branch probes until existing branch mode', (
    tester,
  ) async {
    var branchExistsCalls = 0;

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>['main', 'origin/main'],
      checkBranchExists: (_, branch) async {
        branchExistsCalls += 1;
        return branch == 'main';
      },
      onSubmit: (_) {},
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    expect(branchExistsCalls, 0);

    await tester.tap(find.text('Existing Branch'));
    await tester.pumpAndSettle();

    expect(branchExistsCalls, 2);
    expect(find.text('main (default)'), findsOneWidget);
    expect(find.text('origin/main (default)'), findsNothing);
  });

  testWidgets(
    'manual source branch input clears errors and can submit from the source field',
    (tester) async {
      MockSubmitResult? result;

      await _pumpDialogLauncher(
        tester,
        projects: <Project>[_project()],
        loadBranches: (_) async => const <String>[],
        onSubmit: (val) => result = val,
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      // Tap Continue to validate Step 1 branch exists
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      // We should remain in Step 1 due to validation or see error if we try to proceed without a branch
      final sourceField = find.widgetWithText(TextField, 'Source Branch');
      await tester.enterText(sourceField, 'release/manual');
      await tester.pumpAndSettle();

      // Tap Continue to go to Step 2
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();

      // Now we are in Step 2, test validation errors
      await tester.tap(find.text('Create Workspace'));
      await tester.pumpAndSettle();
      expect(find.text('New branch name is required'), findsOneWidget);

      final newBranchField = find.widgetWithText(
        TextField,
        'New Branch Name *',
      );
      await tester.enterText(newBranchField, 'feature/manual-source-submit');
      await tester.pumpAndSettle();

      await tester.tap(find.text('Create Workspace'));
      await tester.pumpAndSettle();

      expect(result, isNotNull);
      expect(result!.sourceBranch, 'release/manual');
      expect(result!.newBranchName, 'feature/manual-source-submit');
    },
  );

  testWidgets('submits from the new-branch and name fields', (tester) async {
    final results = <MockSubmitResult?>[];

    await _pumpDialogLauncher(
      tester,
      projects: <Project>[_project()],
      loadBranches: (_) async => const <String>[],
      onSubmit: results.add,
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Source Branch'),
      'develop',
    );
    await tester.pumpAndSettle();

    // Go to Step 2
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    final newBranchField = find.widgetWithText(TextField, 'New Branch Name *');
    await tester.enterText(newBranchField, 'feature/new-branch-submit');
    await tester.pumpAndSettle();
    await tester.tap(newBranchField);
    await tester.pump();
    await tester.testTextInput.receiveAction(.done);
    await tester.pumpAndSettle();

    expect(results.single, isNotNull);
    expect(results.single!.newBranchName, 'feature/new-branch-submit');

    results.clear();
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Source Branch'),
      'develop',
    );
    await tester.pumpAndSettle();

    // Go to Step 2
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();

    await tester.enterText(
      find.widgetWithText(TextField, 'New Branch Name *'),
      'feature/name-submit',
    );
    await tester.pumpAndSettle();
    final nameField = find.widgetWithText(
      TextField,
      'Workspace Name (Optional)',
    );
    await tester.enterText(nameField, 'Named workspace');
    await tester.pumpAndSettle();
    await tester.tap(nameField);
    await tester.pump();
    await tester.testTextInput.receiveAction(.done);
    await tester.pumpAndSettle();

    expect(results.single, isNotNull);
    expect(results.single!.name, 'Named workspace');
  });
}
