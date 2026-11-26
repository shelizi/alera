import 'package:alera/src/features/workbench/presentation/workspace_explorer.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('an empty changed-path batch still refreshes quick-open', () {
    expect(
      workspaceExplorerWatcherTouchesSearch(
        changedRelativePaths: const [],
        excludedDirectories: const {'node_modules'},
      ),
      isTrue,
    );
  });

  test('only excluded metadata directories do not invalidate quick-open', () {
    expect(
      workspaceExplorerWatcherTouchesSearch(
        changedRelativePaths: const [
          '.git/objects/pack/1',
          'src/.HG/store/cache',
          'dist/generated.js',
        ],
        excludedDirectories: const {'dist'},
      ),
      isFalse,
    );
  });

  test(
    'one relevant source change invalidates even when mixed with ignored paths',
    () {
      expect(
        workspaceExplorerWatcherTouchesSearch(
          changedRelativePaths: const [
            '.svn/wc.db',
            'node_modules/pkg/index.js',
            'src/my_widget.dart',
          ],
          excludedDirectories: const {'node_modules'},
        ),
        isTrue,
      );
    },
  );

  test('ignore and worktree metadata changes reconcile the index', () {
    for (final path in [
      '',
      '.git',
      '.git/info',
      '.git/info/exclude',
      '.git/config',
      '.git/config.worktree',
      '.git/worktrees/new-worktree/gitdir',
      'src/.gitignore',
      'src/.ignore',
    ]) {
      expect(
        workspaceExplorerWatcherTouchesSearch(
          changedRelativePaths: [path],
          excludedDirectories: const {'node_modules'},
        ),
        isTrue,
        reason: path,
      );
    }
    expect(
      workspaceExplorerWatcherTouchesSearch(
        changedRelativePaths: const [
          'node_modules/pkg/.git/config',
          '.git/objects/pack/a',
        ],
        excludedDirectories: const {'node_modules'},
      ),
      isFalse,
    );
  });

  test('similarly named real folders are not excluded by substring', () {
    expect(
      workspaceExplorerWatcherTouchesSearch(
        changedRelativePaths: const ['my.git-project/src.dart'],
        excludedDirectories: const {'.git'},
      ),
      isTrue,
    );
  });
}
