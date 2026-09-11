part of 'settings_dialog_test.dart';

void _registerSettingsDialogProjectTests() {
  testWidgets('opens project settings with the requested project selected', (
    tester,
  ) async {
    final projects = <Project>[
      Project(
        id: 'project-1',
        name: 'First Project',
        repoPath: '/repo/first',
        createdAt: .utc(2026, 6, 27),
        updatedAt: .utc(2026, 6, 27),
      ),
      Project(
        id: 'project-2',
        name: 'Second Project',
        repoPath: '/repo/second',
        createdAt: .utc(2026, 6, 27),
        updatedAt: .utc(2026, 6, 27),
      ),
    ];
    final configRepository = FakeProjectConfigRepository();
    addTearDown(configRepository.dispose);
    final configService = ProjectConfigService(
      repository: configRepository,
      fileStore: FakeProjectConfigFileStore(),
      now: () => DateTime.utc(2026, 6, 27),
    );

    await _pumpSettingsDialog(
      tester,
      initialSectionId: 'projects',
      initialProjectId: 'project-2',
      extraOverrides: <dynamic>[
        projectRepositoryProvider.overrideWithValue(
          _FakeProjectRepository(projects),
        ),
        projectConfigServiceProvider.overrideWithValue(configService),
      ],
    );
    await tester.pumpAndSettle();

    expect(find.text('Projects'), findsWidgets);
    expect(find.text('/repo/second'), findsOneWidget);
    expect(find.text('/repo/first'), findsNothing);
  });

  testWidgets('edits project setup config overrides', (tester) async {
    final project = Project(
      id: 'project-1',
      name: 'Alera',
      repoPath: '/repo/alera',
      createdAt: .utc(2026, 6, 27),
      updatedAt: .utc(2026, 6, 27),
    );
    final configRepository = FakeProjectConfigRepository();
    addTearDown(configRepository.dispose);
    final configService = ProjectConfigService(
      repository: configRepository,
      fileStore: FakeProjectConfigFileStore(),
      now: () => DateTime.utc(2026, 6, 27),
    );
    await _pumpSettingsDialog(
      tester,
      extraOverrides: <dynamic>[
        projectRepositoryProvider.overrideWithValue(
          _FakeProjectRepository(<Project>[project]),
        ),
        projectConfigServiceProvider.overrideWithValue(configService),
      ],
    );

    await tester.tap(find.text('Projects').first);
    await tester.pumpAndSettle();

    expect(find.text('Alera'), findsWidgets);
    expect(find.text('Add Copy Rule'), findsOneWidget);

    await tester.ensureVisible(find.text('Add Copy Rule'));
    await tester.pump();
    await tester.tap(find.text('Add Copy Rule'));
    await tester.pump();
    await tester.enterText(
      find.descendant(
        of: find.byKey(const ValueKey<String>('copy-rule-from-field')),
        matching: find.byType(TextField),
      ),
      '.env',
    );
    await tester.pump();
    await tester.enterText(
      find.descendant(
        of: find.byKey(const ValueKey<String>('copy-rule-to-field')),
        matching: find.byType(TextField),
      ),
      '.env.local',
    );
    await tester.pump();
    await tester.ensureVisible(find.byType(AleraCheckbox).first);
    await tester.pump();
    await tester.tap(find.byType(AleraCheckbox).first);
    await tester.pump();
    await tester.ensureVisible(find.text('Add Setup Command'));
    await tester.pump();
    await tester.tap(find.text('Add Setup Command'));
    await tester.pump();
    await tester.enterText(
      find.descendant(
        of: find.byKey(const ValueKey<String>('setup-command-field')),
        matching: find.byType(TextField),
      ),
      'pnpm install',
    );
    await tester.pump();
    await tester.ensureVisible(find.text('Save Override'));
    await tester.pump();
    await tester.tap(find.text('Save Override'));
    await tester.pump();

    final saved = configRepository.configs[project.id]!;
    expect(saved.worktree.copy.single.from, '.env');
    expect(saved.worktree.copy.single.to, '.env.local');
    expect(saved.worktree.copy.single.overwrite, isTrue);
    expect(saved.worktree.setup, <String>['pnpm install']);
  });

  testWidgets('clears dirty project setup edits when using repo file', (
    tester,
  ) async {
    final project = Project(
      id: 'project-1',
      name: 'Alera',
      repoPath: '/repo/alera',
      createdAt: .utc(2026, 6, 27),
      updatedAt: .utc(2026, 6, 27),
    );
    final configRepository = FakeProjectConfigRepository();
    addTearDown(configRepository.dispose);
    await configRepository.save(
      projectId: project.id,
      config: .empty,
      updatedAt: .utc(2026, 6, 27),
    );
    final configService = ProjectConfigService(
      repository: configRepository,
      fileStore: FakeProjectConfigFileStore(),
      now: () => DateTime.utc(2026, 6, 27),
    );
    await _pumpSettingsDialog(
      tester,
      extraOverrides: <dynamic>[
        projectRepositoryProvider.overrideWithValue(
          _FakeProjectRepository(<Project>[project]),
        ),
        projectConfigServiceProvider.overrideWithValue(configService),
      ],
    );

    await tester.tap(find.text('Projects').first);
    await tester.pumpAndSettle();
    expect(find.text('UI Override'), findsWidgets);

    await tester.ensureVisible(find.text('Add Setup Command'));
    await tester.pump();
    await tester.tap(find.text('Add Setup Command'));
    await tester.pump();
    await tester.enterText(
      find.descendant(
        of: find.byKey(const ValueKey<String>('setup-command-field')),
        matching: find.byType(TextField),
      ),
      'pnpm install',
    );
    await tester.pump();
    expect(find.text('pnpm install'), findsOneWidget);

    await tester.ensureVisible(find.text('Use Repo File'));
    await tester.pump();
    await tester.tap(find.text('Use Repo File'));
    await tester.pumpAndSettle();

    expect(configRepository.configs.containsKey(project.id), isFalse);
    expect(find.text('None'), findsWidgets);
    expect(find.text('No setup commands'), findsOneWidget);
    expect(find.text('pnpm install'), findsNothing);
  });
}
