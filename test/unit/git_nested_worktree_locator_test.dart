import 'dart:io';

import 'package:alera/src/features/language_intelligence/infra/git_nested_worktree_locator.dart';
import 'package:alera/src/shared/infra/git/git_worktree_entry.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  final root = Platform.isWindows ? r'E:\work\repo' : '/work/repo';
  String under(String base, List<String> parts) => p.joinAll([base, ...parts]);

  test('returns only worktrees strictly inside the workspace root', () async {
    String? askedFor;
    final locator = GitNestedWorktreeLocator((repoPath) async {
      askedFor = repoPath;
      return <GitWorktreeEntry>[
        GitWorktreeEntry(path: root, branch: 'main'),
        GitWorktreeEntry(
          path: under(root, ['.worktrees', 'feature']),
          branch: 'feature',
        ),
        GitWorktreeEntry(
          path: under(root, ['.claude', 'worktrees', 'perf']),
          branch: 'perf',
        ),
        GitWorktreeEntry(
          path: under(p.dirname(root), ['.worktrees', 'sibling']),
          branch: 'sibling',
        ),
        GitWorktreeEntry(path: '$root-copy', branch: 'copy'),
      ];
    });

    expect(await locator.nestedCheckouts(root), <String>[
      under(root, ['.worktrees', 'feature']),
      under(root, ['.claude', 'worktrees', 'perf']),
    ]);
    expect(askedFor, root);
  });

  test(
    'a linked-worktree workspace does not list the main checkout or itself',
    () async {
      final linked = under(root, ['.worktrees', 'feature']);
      final locator = GitNestedWorktreeLocator(
        (_) async => <GitWorktreeEntry>[
          GitWorktreeEntry(path: root, branch: 'main'),
          GitWorktreeEntry(path: linked, branch: 'feature'),
        ],
      );

      expect(await locator.nestedCheckouts(linked), isEmpty);
    },
  );
}
