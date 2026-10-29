import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const languages = <AppLanguage>[
    AppLanguage.traditionalChinese,
    AppLanguage.simplifiedChinese,
    AppLanguage.japanese,
  ];
  const menuKeys = <String>[
    'Compare with Branch...',
    'Copy Branch Name',
    'Copy Tag Name',
    'Copy Commit Hash',
    'Copy Commit Subject',
    'Switch to Branch',
    'Rename Branch...',
    'Delete Branch',
    'Merge into Current Branch',
    'Checkout Remote Branch',
    'Delete Remote Branch',
    'Pull into Current Branch',
    'Open in New Worktree...',
    'Push Tag',
    'Delete Tag',
    'Stash Changes',
    'Discard All Changes',
    'Commit Changes...',
    'Checkout Commit',
    'Revert Commit',
    'Drop Commit',
    'Reset Current Branch Here',
    'Soft',
    'Mixed',
    'Hard',
  ];
  const confirmationKeys = <String>[
    'Delete Branch?',
    'Force Delete Branch?',
    'Delete Tag?',
    'Delete Forcefully',
    'Discard All Changes?',
    'Checkout Commit?',
    'Revert Commit?',
    'Reset Current Branch?',
    'Detaches HEAD at {commit} "{subject}". '
        'Commits made in this state do not belong to a branch.',
    'Moves the current branch to {commit} "{subject}" and discards all '
        'working tree changes. This cannot be undone.',
    'Branch "{branch}" was not fully merged. Force deletion removes '
        'the branch reference and cannot be undone.',
    'Merge {branch} into {current}?',
    'Rebase Current Branch onto {branch}?',
  ];

  for (final language in languages) {
    test('$language has all Git history right-click translations', () {
      final keys = AleraLocalizations.staticSourceKeysFor(language);
      for (final key in <String>[...menuKeys, ...confirmationKeys]) {
        expect(keys, contains(key), reason: '$language lacks "$key"');
      }
    });
  }

  test('locale templates reorder values without translating user content', () {
    const source = 'Delete {branch} on {remote}?';
    const values = <String, String>{
      'branch': 'feature/{remote}/test',
      'remote': 'origin',
    };
    expect(
      const AleraLocalizations(Locale('en')).translateTemplate(source, values),
      'Delete feature/{remote}/test on origin?',
    );
    expect(
      const AleraLocalizations(Locale('zh', 'TW'))
          .translateTemplate(source, values),
      '要從 origin 刪除分支 feature/{remote}/test 嗎？',
    );
    expect(
      const AleraLocalizations(Locale('zh', 'CN'))
          .translateTemplate(source, values),
      '要从 origin 删除分支 feature/{remote}/test 吗？',
    );
    expect(
      const AleraLocalizations(Locale('ja', 'JP'))
          .translateTemplate(source, values),
      'origin のブランチ feature/{remote}/test を削除しますか？',
    );
  });

  test('destructive reset confirmation retains commit ID and subject', () {
    const message =
        'Moves the current branch to {commit} "{subject}" '
        'and discards all working tree changes. This cannot be undone.';
    const values = <String, String>{
      'commit': 'abc1234',
      'subject': 'fix: keep {commit} "value"',
    };
    for (final locale in <Locale>[
      const Locale('zh', 'TW'),
      const Locale('zh', 'CN'),
      const Locale('ja', 'JP'),
    ]) {
      final translated = AleraLocalizations(locale)
          .translateTemplate(message, values);
      expect(translated, contains('abc1234'));
      expect(translated, contains('fix: keep {commit} "value"'));
      expect(translated, isNot(message));
    }
  });
}
