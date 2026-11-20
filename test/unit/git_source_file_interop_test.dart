import 'dart:io';

import 'package:alera/src/features/workbench/application/git_source_file_interop.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

GitChangeEntry _change(String path, GitChangeArea area) =>
    GitChangeEntry(path: path, area: area, status: GitChangeStatus.modified);

void main() {
  test(
    'native Cut refuses saved open editor paths under a selected folder',
    () {
      final editors = EditorSessionRegistry();
      addTearDown(editors.dispose);
      editors
          .documentFor('open-tab')
          .attachFile(
            workspacePath: 'C:/work/project',
            relativePath: 'src/feature/file.dart',
          );
      expect(
        editors.hasOpenFilesUnder(
          workspacePath: 'C:/work/project',
          relativePaths: <String>['src'],
        ),
        isTrue,
      );
      expect(
        editors.hasOpenFilesUnder(
          workspacePath: 'C:/work/project',
          relativePaths: <String>['unrelated'],
        ),
        isFalse,
      );
      editors.forget('open-tab');
      expect(
        editors.hasOpenFilesUnder(
          workspacePath: 'C:/work/project',
          relativePaths: <String>['src'],
        ),
        isFalse,
      );
    },
  );

  test('tree selects folders and only visible rows after collapse', () {
    final groups = GitChangeGroup.fromEntries(<GitChangeEntry>[
      _change('src/ui/a.dart', GitChangeArea.unstaged),
      _change('src/lib.dart', GitChangeArea.unstaged),
      _change('readme.md', GitChangeArea.staged),
    ]);
    final expanded = visibleGitSourceFileItems(
      groups: groups,
      viewMode: GitDiffViewMode.tree,
      collapsedSections: <String>{},
      collapsedTreeNodes: <String>{},
    );
    expect(expanded.any((item) => item.id == 'folder:unstaged:src'), isTrue);
    expect(
      expanded.any((item) => item.id == 'unstaged::src/ui/a.dart'),
      isTrue,
    );
    final collapsed = visibleGitSourceFileItems(
      groups: groups,
      viewMode: GitDiffViewMode.tree,
      collapsedSections: <String>{},
      collapsedTreeNodes: <String>{'folder:unstaged:src'},
    );
    expect(collapsed.any((item) => item.id == 'folder:unstaged:src'), isTrue);
    expect(
      collapsed.any((item) => item.relativePath == 'src/ui/a.dart'),
      isFalse,
    );
    expect(collapsed.any((item) => item.relativePath == 'readme.md'), isTrue);
  });

  test('selected parent subsumes descendants and staged duplicates', () {
    final visible = <GitSourceFileItem>[
      const GitSourceFileItem(
        id: 'folder:unstaged:src',
        relativePath: 'src',
        isDirectory: true,
      ),
      const GitSourceFileItem(
        id: 'unstaged::src/a.dart',
        relativePath: 'src/a.dart',
        isDirectory: false,
      ),
      const GitSourceFileItem(
        id: 'staged::src/a.dart',
        relativePath: 'src/a.dart',
        isDirectory: false,
      ),
      const GitSourceFileItem(
        id: 'untracked::notes.txt',
        relativePath: 'notes.txt',
        isDirectory: false,
      ),
    ];
    final selected = <String>{
      'folder:unstaged:src',
      'unstaged::src/a.dart',
      'staged::src/a.dart',
      'untracked::notes.txt',
    };
    expect(
      gitSourceSelectedPaths(visible: visible, selectedIds: selected),
      <String>['notes.txt', 'src'],
    );
    expect(
      gitSourceSelectedPaths(
        visible: visible,
        selectedIds: selected,
        clickedId: 'unselected',
      ),
      isEmpty,
    );
    final hidden = visible
        .where((item) => item.id != 'folder:unstaged:src')
        .toList();
    expect(
      gitSourceSelectedPaths(visible: hidden, selectedIds: selected),
      <String>['notes.txt', 'src/a.dart'],
    );
  });

  test(
    'external clipboard validation blocks git metadata and missing files',
    () async {
      final root = await Directory.systemTemp.createTemp('alera-git-interop-');
      addTearDown(() => root.delete(recursive: true));
      await Directory(p.join(root.path, 'src')).create();
      await File(p.join(root.path, 'src', 'a.dart')).writeAsString('source');
      final guard = const GitSourceFileGuard();
      final paths = await guard.checkedSourcePaths(
        workspacePath: root.path,
        relativePaths: <String>['src/a.dart'],
        limit: gitSourceClipboardLimit,
      );
      expect(paths, hasLength(1));
      expect(await File(paths.single).readAsString(), 'source');
      await Directory(p.join(root.path, 'src', '.git')).create();
      await expectLater(
        guard.checkedSourcePaths(
          workspacePath: root.path,
          relativePaths: <String>['src'],
          limit: gitSourceClipboardLimit,
        ),
        throwsA(isA<FileSystemException>()),
      );
      await expectLater(
        guard.checkedSourcePaths(
          workspacePath: root.path,
          relativePaths: <String>['../outside'],
          limit: gitSourceClipboardLimit,
        ),
        throwsA(isA<FileSystemException>()),
      );
    },
  );

  test(
    'collision and descendant folder preflight do not mutate sources',
    () async {
      final root = await Directory.systemTemp.createTemp('alera-git-paste-');
      addTearDown(() => root.delete(recursive: true));
      await Directory(p.join(root.path, 'from')).create();
      await Directory(p.join(root.path, 'dest')).create();
      await File(p.join(root.path, 'from', 'a.txt')).writeAsString('original');
      await File(p.join(root.path, 'dest', 'a.txt')).writeAsString('occupied');
      final guard = const GitSourceFileGuard();
      await expectLater(
        guard.preflightDestination(
          absoluteSources: <String>[p.join(root.path, 'from', 'a.txt')],
          destinationFolder: p.join(root.path, 'dest'),
        ),
        throwsA(isA<FileSystemException>()),
      );
      await expectLater(
        guard.preflightDestination(
          absoluteSources: <String>[p.join(root.path, 'from')],
          destinationFolder: p.join(root.path, 'from'),
        ),
        throwsA(isA<FileSystemException>()),
      );
      expect(
        await File(p.join(root.path, 'from', 'a.txt')).readAsString(),
        'original',
      );
    },
  );
}
