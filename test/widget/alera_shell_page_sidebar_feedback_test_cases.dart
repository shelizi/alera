part of 'alera_shell_page_test.dart';

Future<Directory> _configureWorkspaceRemovalLog(WidgetTester tester) async {
  final directory = (await tester.runAsync(() async {
    final root = await Directory.systemTemp.createTemp('alera-removal-log-');
    await AppLogger.configure(directory: root, consoleWriter: (_) {});
    return root;
  }))!;
  addTearDown(() async {
    await AppLogger.resetForTesting();
    await directory.delete(recursive: true);
  });
  return directory;
}

Future<Map<String, dynamic>> _readWorkspaceRemovalLog(
  WidgetTester tester,
  Directory directory,
) async {
  return (await tester.runAsync(() async {
    await AppLogger.flush();
    final lines = await File(p.join(directory.path, 'alera.log')).readAsLines();
    return lines
        .map((line) => jsonDecode(line) as Map<String, dynamic>)
        .singleWhere((record) => record['logger'] == 'WorkspaceRemoval');
  }))!;
}

void _registerAleraShellSidebarFeedbackTests() {
  testWidgets('active workspace is visually emphasized in the sidebar', (
    tester,
  ) async {
    await _pumpShell(
      tester,
      state: _linkedWorkbenchState(linkedExpanded: true, linkedActive: true),
    );

    final activeContainer = find
        .ancestor(
          of: find.text('Feature login').first,
          matching: find.byType(AnimatedContainer),
        )
        .first;
    final inactiveContainer = find
        .ancestor(
          of: find.text('Main').first,
          matching: find.byType(AnimatedContainer),
        )
        .first;

    final activeDecoration =
        tester.widget<AnimatedContainer>(activeContainer).decoration!
            as BoxDecoration;
    final inactiveDecoration =
        tester.widget<AnimatedContainer>(inactiveContainer).decoration!
            as BoxDecoration;

    expect(activeDecoration.color, AleraTokens.surfaceElevated);
    expect(activeDecoration.border, isA<Border>());
    final activeBorder = activeDecoration.border! as Border;
    expect(activeBorder.top.color, AleraTokens.accent);
    expect(activeBorder.top.width, AleraTokens.strokeThin);
    expect(inactiveDecoration.border, isNull);
  });
  testWidgets('project rename failures surface an error toast event', (
    tester,
  ) async {
    final events = <AleraToastData>[];
    final subscription = AleraToast.stream.listen(events.add);
    addTearDown(subscription.cancel);
    final state = _linkedWorkbenchState(linkedExpanded: true);

    await _pumpShell(
      tester,
      state: state,
      controller: _ShellTestWorkbenchController(
        state,
        renameProjectFailure: StateError('rename failed'),
      ),
    );

    await tester.tapAt(
      tester.getCenter(find.text('Alera').last),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Project Name'),
      'New',
    );
    await tester.tap(find.text('Rename'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(events.last.message, 'Bad state: rename failed');
  });

  testWidgets('workspace rename failures surface an error toast event', (
    tester,
  ) async {
    final events = <AleraToastData>[];
    final subscription = AleraToast.stream.listen(events.add);
    addTearDown(subscription.cancel);
    final state = _linkedWorkbenchState(linkedExpanded: true);

    await _pumpShell(
      tester,
      state: state,
      controller: _ShellTestWorkbenchController(
        state,
        renameWorkspaceFailure: StateError('rename workspace failed'),
      ),
    );

    await tester.tapAt(
      tester.getCenter(find.text('Feature login').first),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Workspace Name'),
      'Renamed',
    );
    await tester.tap(find.text('Rename'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(events.last.message, 'Bad state: rename workspace failed');
  });

  testWidgets('workspace removal failures show and persist error details', (
    tester,
  ) async {
    final directory = await _configureWorkspaceRemovalLog(tester);
    final events = <AleraToastData>[];
    final subscription = AleraToast.stream.listen(events.add);
    addTearDown(subscription.cancel);
    final state = _linkedWorkbenchState(linkedExpanded: true);

    final harness = await _pumpShell(
      tester,
      state: state,
      controller: _ShellTestWorkbenchController(
        state,
        deleteWorkspaceFailure: StateError('delete workspace failed'),
      ),
    );

    await tester.tapAt(
      tester.getCenter(find.text('Feature login').first),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Remove'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Clean Up'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(find.text('Worktree removal failed'), findsOneWidget);
    final details = find.byWidgetPredicate(
      (widget) =>
          widget is SelectableText &&
          (widget.data?.contains('Bad state: delete workspace failed') ??
              false),
    );
    expect(details, findsOneWidget);
    final text = tester.widget<SelectableText>(details).data!;
    expect(text, contains('Feature login'));
    expect(text, contains(state.workspacesFor('project-1').last.path));
    expect(events.where((event) => event.tone == .success), isEmpty);
    expect(
      harness.controller.state.workspacesFor('project-1').map((w) => w.id),
      contains('workspace-2'),
    );
    await tester.pump(const Duration(seconds: 10));
    expect(find.text('Worktree removal failed'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Close'));
    await tester.pumpAndSettle();
    expect(find.text('Worktree removal failed'), findsNothing);
    expect(find.text('Feature login'), findsWidgets);
    final log = await _readWorkspaceRemovalLog(tester, directory);
    expect(log['level'], 'SEVERE');
    expect(DateTime.tryParse(log['ts'] as String), isNotNull);
    expect(log['msg'], contains('stage=removeWorkspace'));
    expect(log['msg'], contains('workspaceId=workspace-2'));
    expect(log['msg'], contains(state.workspacesFor('project-1').last.path));
    expect(log['error'], 'Bad state: delete workspace failed');
    expect(log['stack'], isNotEmpty);
  });

  for (final blocked in [false, true]) {
    testWidgets(
      'workspace removal logs preflight ${blocked ? 'blockers' : 'errors'}',
      (tester) async {
        final directory = await _configureWorkspaceRemovalLog(tester);
        final state = _linkedWorkbenchState(linkedExpanded: true);
        final harness = await _pumpShell(
          tester,
          state: state,
          managedWorkspaceRuntime: _FakeManagedWorkspaceRuntime(
            storageImpactFailure: blocked
                ? null
                : StateError('storage RPC failed'),
            blockers: blocked
                ? const ['Workspace has an active automation']
                : const [],
          ),
        );
        await tester.tapAt(
          tester.getCenter(find.text('Feature login').first),
          buttons: kSecondaryMouseButton,
        );
        await tester.pumpAndSettle();
        await tester.tap(find.text('Remove'));
        await tester.pumpAndSettle();
        expect(
          find.text(
            blocked ? 'Cleanup Unavailable' : 'Worktree removal failed',
          ),
          findsOneWidget,
        );
        expect(harness.runtime.closedWorkspaceIds, isEmpty);
        await tester.tap(find.widgetWithText(FilledButton, 'Close'));
        await tester.pumpAndSettle();
        final log = await _readWorkspaceRemovalLog(tester, directory);
        expect(log['level'], blocked ? 'WARNING' : 'SEVERE');
        expect(
          log['msg'],
          contains(blocked ? 'stage=storageBlocked' : 'stage=storageImpact'),
        );
        expect(
          log['msg'],
          contains(state.workspacesFor('project-1').last.path),
        );
        expect(
          log['error'],
          contains(
            blocked
                ? 'Workspace has an active automation'
                : 'storage RPC failed',
          ),
        );
        if (!blocked) expect(log['stack'], isNotEmpty);
      },
    );
  }

  testWidgets('project removal failures surface an error toast event', (
    tester,
  ) async {
    final events = <AleraToastData>[];
    final subscription = AleraToast.stream.listen(events.add);
    addTearDown(subscription.cancel);
    final state = _linkedWorkbenchState(linkedExpanded: true);

    final harness = await _pumpShell(
      tester,
      state: state,
      controller: _ShellTestWorkbenchController(
        state,
        removeProjectFailure: StateError('remove project failed'),
      ),
    );

    await tester.tapAt(
      tester.getCenter(find.text('Alera').last),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Remove Project'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Remove'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(events.last.message, 'Bad state: remove project failed');
    expect(
      harness.runtime.closedWorkspaceIds,
      containsAll(<String>['workspace-1', 'workspace-2']),
    );
  });

  testWidgets('sidebar agent close failures surface an error toast event', (
    tester,
  ) async {
    const prompt = 'Review linked workspace';
    const description = 'Codex · Waiting for input';
    final events = <AleraToastData>[];
    final subscription = AleraToast.stream.listen(events.add);
    addTearDown(subscription.cancel);
    final state = _linkedWorkbenchState(linkedExpanded: true);

    final harness = await _pumpShell(
      tester,
      state: state,
      controller: _ShellTestWorkbenchController(
        state,
        closeWorkspaceTabFailure: StateError('close tab failed'),
      ),
      agentStatuses: <String, AgentStatusEntry>{
        'tab-2': _agentStatusEntry(
          terminalSessionId: 'tab-2',
          workspaceId: 'workspace-2',
          tabId: 'tab-2',
          state: .waiting,
          prompt: prompt,
        ),
      },
    );
    final mouse = await tester.createGesture(kind: .mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer();
    await mouse.moveTo(tester.getCenter(find.text(description)));
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Close Terminal').first);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));

    expect(harness.runtime.closedTabIds, isEmpty);
    expect(events.last.message, 'Bad state: close tab failed');
  });

  testWidgets('hovering sidebar rows updates their highlight state', (
    tester,
  ) async {
    const prompt = 'Review linked workspace';
    const description = 'Codex · Waiting for input';
    await _pumpShell(
      tester,
      state: _linkedWorkbenchState(linkedExpanded: true),
      agentStatuses: <String, AgentStatusEntry>{
        'tab-2': _agentStatusEntry(
          terminalSessionId: 'tab-2',
          workspaceId: 'workspace-2',
          tabId: 'tab-2',
          state: .waiting,
          prompt: prompt,
        ),
      },
    );

    final mouse = await tester.createGesture(kind: .mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer();

    final projectContainer = find
        .ancestor(
          of: find.text('Alera').last,
          matching: find.byType(AnimatedContainer),
        )
        .first;
    final workspaceContainer = find
        .ancestor(
          of: find.text('Feature login').first,
          matching: find.byType(AnimatedContainer),
        )
        .first;
    final terminalContainer = find
        .ancestor(
          of: find.text(description),
          matching: find.byType(AnimatedContainer),
        )
        .first;

    BoxDecoration decorationOf(Finder finder) {
      return tester.widget<AnimatedContainer>(finder).decoration!
          as BoxDecoration;
    }

    expect(decorationOf(projectContainer).color, Colors.transparent);
    await mouse.moveTo(tester.getCenter(find.text('Alera').last));
    await tester.pumpAndSettle();
    expect(decorationOf(projectContainer).color, AleraTokens.surface);
    await mouse.moveTo(const Offset(0, 0));
    await tester.pumpAndSettle();
    expect(decorationOf(projectContainer).color, Colors.transparent);

    expect(decorationOf(workspaceContainer).color, Colors.transparent);
    await mouse.moveTo(tester.getCenter(find.text('Feature login').first));
    await tester.pumpAndSettle();
    expect(decorationOf(workspaceContainer).color, AleraTokens.surface);
    await mouse.moveTo(const Offset(0, 0));
    await tester.pumpAndSettle();
    expect(decorationOf(workspaceContainer).color, Colors.transparent);

    expect(decorationOf(terminalContainer).color, Colors.transparent);
    await mouse.moveTo(tester.getCenter(find.text(description)));
    await tester.pumpAndSettle();
    expect(decorationOf(terminalContainer).color, AleraTokens.surface);
    await mouse.moveTo(const Offset(0, 0));
    await tester.pumpAndSettle();
    expect(decorationOf(terminalContainer).color, Colors.transparent);
  });
}
