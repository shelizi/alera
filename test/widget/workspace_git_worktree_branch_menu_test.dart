import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_menus.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('existing worktree replaces branch switch in Git Graph menu', (
    tester,
  ) async {
    GitHistoryRefMenuAction? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await showGitHistoryRefMenuForKind(
                  context,
                  const GitHistoryItemRef(
                    id: 'refs/heads/feature',
                    name: 'feature',
                    category: GitHistoryRefCategory.branches,
                  ),
                  const Offset(20, 20),
                  kind: GitHistoryRefMenuKind.localBranch,
                  isCurrentBranch: false,
                  hasExistingWorktree: true,
                );
              },
              child: const Text('Show branch menu'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Show branch menu'));
    await tester.pumpAndSettle();
    expect(find.text('Switch to Worktree'), findsOneWidget);
    expect(find.text('Switch to Branch'), findsNothing);
    final openNew = tester.widget<AleraDropdownEntry<GitHistoryRefMenuAction>>(
      find.widgetWithText(
        AleraDropdownEntry<GitHistoryRefMenuAction>,
        'Open in New Worktree',
      ),
    );
    expect(openNew.enabled, isFalse);
    await tester.tap(find.text('Switch to Worktree'));
    await tester.pumpAndSettle();
    expect(result, GitHistoryRefMenuAction.switchBranch);
  });
}
