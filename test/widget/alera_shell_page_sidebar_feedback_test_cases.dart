part of 'alera_shell_page_test.dart';

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

  testWidgets('workspace removal failures surface an error toast event', (
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

    expect(events.last.message, 'Bad state: delete workspace failed');
  });

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
