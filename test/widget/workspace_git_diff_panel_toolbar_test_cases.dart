part of 'workspace_git_diff_panel_test.dart';

void _registerWorkspaceGitDiffPanelToolbarTests() {
  testWidgets('source control actions align with the content edges', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitRepositoryStateResult = const GitRepositoryState(
        branch: 'main',
        upstream: 'origin/main',
      )
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/new.dart',
            area: .untracked,
            status: .untracked,
          ),
        ],
      );

    await _pumpPanel(tester, backend: backend);
    await tester.pumpAndSettle();

    final splitButton = find.ancestor(
      of: find.text('Stage All'),
      matching: find.byWidgetPredicate(
        (widget) => widget is SizedBox && widget.height == 28,
      ),
    );
    expect(splitButton, findsOneWidget);
    final splitButtonRect = tester.getRect(splitButton);
    final messageFieldRect = tester.getRect(_messageField());
    final lastActionRect = tester.getRect(
      find.byTooltip('Generate commit message with AI'),
    );
    expect(splitButtonRect.height, 28);
    expect(splitButtonRect.left, closeTo(messageFieldRect.left, 0.1));
    expect(splitButtonRect.right, closeTo(messageFieldRect.right, 0.1));
    expect(lastActionRect.right, closeTo(messageFieldRect.right, 0.1));

    final primaryAction = find.ancestor(
      of: find.text('Stage All'),
      matching: find.byType(InkWell),
    );
    expect(
      tester.widget<InkWell>(primaryAction).mouseCursor,
      SystemMouseCursors.click,
    );

    final dropdownToggle = find.ancestor(
      of: find.descendant(
        of: splitButton,
        matching: find.byIcon(AleraIcons.chevronDown),
      ),
      matching: find.byType(InkWell),
    );
    expect(
      tester.widget<InkWell>(dropdownToggle).mouseCursor,
      SystemMouseCursors.click,
    );
  });

  testWidgets('AI commit message action is hidden when AI Assist is disabled', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/staged.dart',
            area: .staged,
            status: .modified,
          ),
        ],
      );

    await _pumpPanel(
      tester,
      backend: backend,
      settings: AleraSettings.defaults.copyWith(
        aiAssist: AiAssistSettings.defaults.copyWith(enabled: false),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('Generate commit message with AI'), findsNothing);
  });

  testWidgets(
    'AI commit message action stays visible at the default sidebar width',
    (tester) async {
      final backend = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: <GitChangeEntry>[
            GitChangeEntry(
              path: 'lib/staged.dart',
              area: .staged,
              status: .modified,
            ),
          ],
        );

      await _pumpPanel(tester, backend: backend, width: 280);
      await tester.pumpAndSettle();

      final button = find.byTooltip('Generate commit message with AI');
      expect(button, findsOneWidget);
      final viewport = tester.getRect(
        find.ancestor(of: button, matching: find.byType(SingleChildScrollView)),
      );
      final buttonRect = tester.getRect(button);
      expect(buttonRect.left, greaterThanOrEqualTo(viewport.left - 0.5));
      expect(buttonRect.right, lessThanOrEqualTo(viewport.right + 0.5));
    },
  );

  testWidgets('commit message field has extra top padding', (tester) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(entries: <GitChangeEntry>[]);

    await _pumpPanel(tester, backend: backend);
    await tester.pumpAndSettle();

    final messageField = tester.widget<TextField>(_messageField());
    expect(messageField.decoration?.suffixIcon, isNull);
    expect(
      messageField.decoration?.contentPadding,
      const EdgeInsets.fromLTRB(
        AleraTokens.space8,
        AleraTokens.space16,
        AleraTokens.space48,
        AleraTokens.space8,
      ),
    );
    final fieldRect = tester.getRect(_messageField());
    final dictationRect = tester.getRect(
      find.byKey(const ValueKey<String>('source-control-dictation-control')),
    );
    expect(
      dictationRect.top - fieldRect.top,
      lessThanOrEqualTo(AleraTokens.space12),
    );
    expect(
      fieldRect.right - dictationRect.right,
      lessThanOrEqualTo(AleraTokens.space12),
    );
  });
}
