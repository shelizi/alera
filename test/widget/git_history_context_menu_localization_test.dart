import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_actions.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_dialogs.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'Git Graph commit right-click menu translates Traditional Chinese',
    (tester) async {
      final commit = GitHistoryItem(
        id: 'abc123456',
        parentIds: const ['parent1'],
        subject: 'Fix test',
        message: 'Fix test',
        references: const [],
      );
      await _pumpMenu(tester, const Locale('zh', 'TW'), (context) async {
        await showGitHistoryCommitMenu(
          context,
          commit,
          const Offset(20, 20),
          currentBranchName: 'main',
        );
      });
      await tester.tap(find.text('Open Menu'));
      await tester.pumpAndSettle();
      expect(find.text('複製 Commit Hash'), findsOneWidget);
      expect(find.text('在此建立 Branch…'), findsOneWidget);
      expect(find.text('在此重設目前 Branch'), findsOneWidget);
    },
  );

  testWidgets('Git Graph ref right-click menu translates Simplified Chinese', (
    tester,
  ) async {
    const ref = GitHistoryItemRef(
      id: 'refs/heads/feature/example',
      name: 'feature/example',
      revision: 'abc123456',
      category: GitHistoryRefCategory.branches,
    );
    await _pumpMenu(tester, const Locale('zh', 'CN'), (context) async {
      await showGitHistoryRefMenuForKind(
        context,
        ref,
        const Offset(20, 20),
        kind: GitHistoryRefMenuKind.localBranch,
        isCurrentBranch: false,
        canCompareBranches: true,
      );
    });
    await tester.tap(find.text('Open Menu'));
    await tester.pumpAndSettle();
    expect(find.text('与其他分支比较…'), findsOneWidget);
    expect(find.text('重命名分支…'), findsOneWidget);
    expect(find.text('将当前分支 Rebase 到 feature/example'), findsOneWidget);
  });

  testWidgets('Git Graph boundary right-click menu translates Japanese', (
    tester,
  ) async {
    await _pumpMenu(tester, const Locale('ja', 'JP'), (context) async {
      await showGitHistoryBoundaryMenu(context, const Offset(20, 20));
    });
    await tester.tap(find.text('Open Menu'));
    await tester.pumpAndSettle();
    expect(find.text('変更を Stash'), findsOneWidget);
    expect(find.text('すべての変更を破棄'), findsOneWidget);
    expect(find.text('変更を Commit…'), findsOneWidget);
  });

  testWidgets('destructive branch confirmation localizes dynamic branch name', (
    tester,
  ) async {
    await _pumpMenu(tester, const Locale('zh', 'TW'), (context) async {
      await showGitHistoryDeleteBranchConfirmation(context, 'feature/example');
    });
    await tester.tap(find.text('Open Menu'));
    await tester.pumpAndSettle();
    expect(find.text('要刪除此分支嗎？'), findsOneWidget);
    expect(find.text('要刪除本機分支「feature/example」嗎？'), findsOneWidget);
    expect(find.text('刪除'), findsOneWidget);
  });
}

Future<void> _pumpMenu(
  WidgetTester tester,
  Locale locale,
  Future<void> Function(BuildContext context) open,
) {
  return tester.pumpWidget(
    MaterialApp(
      locale: locale,
      supportedLocales: supportedAleraLocales,
      localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
        AleraLocalizationsDelegate(),
        ...GlobalMaterialLocalizations.delegates,
      ],
      home: Scaffold(
        body: Builder(
          builder: (context) => ElevatedButton(
            onPressed: () => open(context),
            child: const Text('Open Menu'),
          ),
        ),
      ),
    ),
  );
}
