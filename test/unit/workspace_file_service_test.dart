import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/shared/infra/git/git_explorer_status.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('WorkspaceFileService.applyGitStatusSnapshot', () {
    const service = WorkspaceFileService();

    test('returns the original empty entry list', () {
      final entries = <native.WorkspaceFileEntry>[];

      final result = service.applyGitStatusSnapshot(
        entries,
        const GitExplorerStatusSnapshot.empty(),
      );

      expect(identical(result, entries), isTrue);
    });

    test('reuses the list when the snapshot does not change any status', () {
      final entry = _workspaceEntry('main.dart');
      final entries = <native.WorkspaceFileEntry>[entry];

      final result = service.applyGitStatusSnapshot(
        entries,
        const GitExplorerStatusSnapshot.empty(),
      );

      expect(identical(result, entries), isTrue);
      expect(identical(result.single, entry), isTrue);
    });

    test('reuses entries whose git status is already applied', () {
      final entry = _workspaceEntry(
        'main.dart',
        gitStatus: native.WorkspaceFileGitStatus.modified,
      );
      final entries = <native.WorkspaceFileEntry>[entry];

      final result = service.applyGitStatusSnapshot(
        entries,
        GitExplorerStatusSnapshot(<String, GitExplorerStatus>{
          'main.dart': GitExplorerStatus.modified,
        }),
      );

      expect(identical(result, entries), isTrue);
      expect(identical(result.single, entry), isTrue);
    });

    test('copies only changed entries and preserves entry metadata', () {
      final changed = _workspaceEntry(
        'src/main.dart',
        isIgnored: true,
        isHidden: true,
        isSymlink: true,
        isProtected: true,
        hasChildrenHint: true,
      );
      final untouched = _workspaceEntry(
        'README.md',
        gitStatus: native.WorkspaceFileGitStatus.added,
      );
      final entries = <native.WorkspaceFileEntry>[changed, untouched];

      final result = service.applyGitStatusSnapshot(
        entries,
        GitExplorerStatusSnapshot(<String, GitExplorerStatus>{
          'src/main.dart': GitExplorerStatus.untracked,
        }),
      );

      expect(identical(result, entries), isFalse);
      expect(identical(result[0], changed), isFalse);
      expect(identical(result[1], untouched), isTrue);
      expect(result[0].relativePath, changed.relativePath);
      expect(result[0].name, changed.name);
      expect(result[0].kind, changed.kind);
      expect(result[0].size, changed.size);
      expect(result[0].modifiedMillis, changed.modifiedMillis);
      expect(result[0].contentToken, changed.contentToken);
      expect(result[0].isIgnored, changed.isIgnored);
      expect(result[0].isHidden, changed.isHidden);
      expect(result[0].isSymlink, changed.isSymlink);
      expect(result[0].isProtected, changed.isProtected);
      expect(result[0].hasChildrenHint, changed.hasChildrenHint);
      expect(result[0].gitStatus, native.WorkspaceFileGitStatus.untracked);
      expect(result[1].gitStatus, native.WorkspaceFileGitStatus.added);
    });
  });

  group('EditorSessionRegistry', () {
    test(
      'forwards language navigation only to the live editor session',
      () async {
        final registry = EditorSessionRegistry();
        final commands = <EditorSessionNavigationCommand>[];
        final handle = EditorSessionHandle(
          isDirty: () => false,
          save: () async {},
          discard: () async {},
          runNavigationCommand: (command) async => commands.add(command),
        );

        registry.register('tab-1', handle);
        await registry.runNavigationCommand(
          'tab-1',
          EditorSessionNavigationCommand.goToDefinition,
        );
        await registry.runNavigationCommand(
          'tab-1',
          EditorSessionNavigationCommand.findReferences,
        );
        expect(commands, <EditorSessionNavigationCommand>[
          EditorSessionNavigationCommand.goToDefinition,
          EditorSessionNavigationCommand.findReferences,
        ]);

        registry.unregister('tab-1', handle);
        await registry.runNavigationCommand(
          'tab-1',
          EditorSessionNavigationCommand.goToDefinition,
        );
        expect(commands, hasLength(2));
      },
    );

    test(
      'keeps dirty document state after the editor widget unregisters',
      () async {
        final registry = EditorSessionRegistry();
        final document = registry.documentFor('tab-1')
          ..acceptLoaded(
            _editorFile(rawContent: 'original', displayContent: 'original'),
          )
          ..updateCurrentText('changed');
        var saveCount = 0;
        final handle = EditorSessionHandle(
          isDirty: () => document.isDirty,
          save: () async {
            saveCount += 1;
          },
          discard: () async {},
        );

        registry.register('tab-1', handle);
        expect(registry.isDirty('tab-1'), isTrue);
        await registry.save('tab-1');
        expect(saveCount, 1);

        registry.unregister('tab-1', handle);
        expect(registry.isDirty('tab-1'), isTrue);
        await registry.save('tab-1');
        expect(saveCount, 1);

        registry.forget('tab-1');
        expect(registry.isDirty('tab-1'), isFalse);
      },
    );

    test('releases a path notifier once nothing references its path', () {
      final registry = EditorSessionRegistry();
      registry
          .documentFor('tab-1')
          .attachFile(
            workspacePath: '/repo/alera',
            relativePath: 'docs/readme.md',
          );
      final notifier = registry.documentChangesForPath(
        workspacePath: '/repo/alera',
        relativePath: 'docs/readme.md',
      );

      registry.forget('tab-1');

      // A rebuilt viewer obtains a fresh notifier: the old one was dropped
      // instead of accumulating one entry per file ever opened.
      final next = registry.documentChangesForPath(
        workspacePath: '/repo/alera',
        relativePath: 'docs/readme.md',
      );
      expect(identical(next, notifier), isFalse);
    });

    test('keeps a path notifier alive while a viewer is listening', () {
      final registry = EditorSessionRegistry();
      registry
          .documentFor('tab-1')
          .attachFile(
            workspacePath: '/repo/alera',
            relativePath: 'docs/readme.md',
          );
      final notifier = registry.documentChangesForPath(
        workspacePath: '/repo/alera',
        relativePath: 'docs/readme.md',
      );
      void listener() {}
      notifier.addListener(listener);

      registry.forget('tab-1');

      final next = registry.documentChangesForPath(
        workspacePath: '/repo/alera',
        relativePath: 'docs/readme.md',
      );
      expect(identical(next, notifier), isTrue);
      notifier.removeListener(listener);
    });

    test('load errors clear dirty and saveable document state', () {
      final registry = EditorSessionRegistry();
      final document = registry.documentFor('tab-1')
        ..acceptLoaded(
          _editorFile(rawContent: 'original', displayContent: 'original'),
        )
        ..updateCurrentText('changed');

      expect(document.isDirty, isTrue);
      expect(document.canSave, isTrue);

      document.acceptLoadError(StateError('failed to load'));

      expect(document.isDirty, isFalse);
      expect(document.canSave, isFalse);
      expect(document.loadedText, isNull);
      expect(document.currentText, isNull);
      expect(document.contentToken, isNull);
      expect(document.loadError, isA<StateError>());
    });

    test('returns dirty editor text for a matching document path', () {
      final registry = EditorSessionRegistry();
      registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        )
        ..acceptLoaded(
          _editorFile(rawContent: '# Saved', displayContent: '# Saved'),
        )
        ..updateCurrentText('# Dirty');

      expect(
        registry.dirtyTextForPath(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        ),
        '# Dirty',
      );
      expect(
        registry.dirtyTextForPath(
          workspacePath: '/repo/alera',
          relativePath: 'docs/other.md',
        ),
        isNull,
      );
    });

    test('does not return clean editor text for a matching document path', () {
      final registry = EditorSessionRegistry();
      registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        )
        ..acceptLoaded(
          _editorFile(rawContent: '# Saved', displayContent: '# Saved'),
        );

      expect(
        registry.dirtyTextForPath(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        ),
        isNull,
      );
    });

    test('does not return dirty text from a document with load errors', () {
      final registry = EditorSessionRegistry();
      registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        )
        ..acceptLoaded(
          _editorFile(rawContent: '# Saved', displayContent: '# Saved'),
        )
        ..updateCurrentText('# Dirty')
        ..acceptLoadError(StateError('failed to load'));

      expect(
        registry.dirtyTextForPath(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        ),
        isNull,
      );
    });

    test('reports whether document text actually changed', () {
      final registry = EditorSessionRegistry();
      var notifications = 0;
      registry.addListener(() => notifications += 1);
      final document = registry.documentFor('tab-1')
        ..acceptLoaded(
          _editorFile(rawContent: 'original', displayContent: 'original'),
        );
      notifications = 0;

      expect(document.updateCurrentText('original'), isFalse);
      expect(notifications, 0);
      expect(document.updateCurrentText('changed'), isTrue);
      expect(notifications, 1);
    });

    test(
      'notifies listeners when document content changes and is forgotten',
      () {
        final registry = EditorSessionRegistry();
        var notifications = 0;
        registry.addListener(() {
          notifications += 1;
        });

        registry.documentFor('tab-1')
          ..attachFile(
            workspacePath: '/repo/alera',
            relativePath: 'docs/readme.md',
          )
          ..acceptLoaded(
            _editorFile(rawContent: '# Saved', displayContent: '# Saved'),
          )
          ..updateCurrentText('# Dirty');
        registry.forget('tab-1');

        expect(notifications, 4);
      },
    );

    test('notifies only listeners for the changed document path', () {
      final registry = EditorSessionRegistry();
      var readmeNotifications = 0;
      var otherNotifications = 0;
      registry
          .documentChangesForPath(
            workspacePath: '/repo/alera',
            relativePath: 'docs/readme.md',
          )
          .addListener(() => readmeNotifications += 1);
      registry
          .documentChangesForPath(
            workspacePath: '/repo/alera',
            relativePath: 'docs/other.md',
          )
          .addListener(() => otherNotifications += 1);

      registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'docs/readme.md',
        )
        ..acceptLoaded(
          _editorFile(rawContent: '# Saved', displayContent: '# Saved'),
        )
        ..updateCurrentText('# Dirty');

      expect(readmeNotifications, 3);
      expect(otherNotifications, 0);
    });

    test(
      'saveAll writes dirty documents without live editor widgets',
      () async {
        final registry = EditorSessionRegistry();
        final service = _FakeWorkspaceFileService();
        registry.documentFor('tab-1')
          ..attachFile(workspacePath: '/repo/alera', relativePath: 'note.txt')
          ..acceptLoaded(
            _editorFile(rawContent: 'original', displayContent: 'original'),
          )
          ..updateCurrentText('changed');

        final savedCount = await registry.saveAll(service);

        expect(savedCount, 1);
        expect(service.writes.single.relativePath, 'note.txt');
        expect(service.writes.single.currentDisplayContent, 'changed');
        expect(service.writes.single.originalRawContent, 'original');
        expect(service.writes.single.originalDisplayContent, 'original');
        expect(registry.isDirty('tab-1'), isFalse);
      },
    );

    test('saveAll preserves raw tabs on unchanged lines', () async {
      final registry = EditorSessionRegistry();
      final service = _FakeWorkspaceFileService();
      registry.documentFor('tab-1')
        ..attachFile(workspacePath: '/repo/alera', relativePath: 'note.txt')
        ..acceptLoaded(
          _editorFile(
            rawContent: '\talpha\n\tbeta\n\tgamma\n',
            displayContent: '    alpha\n    beta\n    gamma\n',
          ),
          tabSize: 4,
        )
        ..updateCurrentText('    alpha\n    beta changed\n    gamma\n');

      final savedCount = await registry.saveAll(service);

      expect(savedCount, 1);
      expect(service.writes.single.relativePath, 'note.txt');
      expect(
        service.writes.single.currentDisplayContent,
        '    alpha\n'
        '    beta changed\n'
        '    gamma\n',
      );
      expect(
        service.writes.single.originalRawContent,
        '\talpha\n'
        '\tbeta\n'
        '\tgamma\n',
      );
      expect(
        service.writes.single.originalDisplayContent,
        '    alpha\n'
        '    beta\n'
        '    gamma\n',
      );
      expect(service.writes.single.tabSize, 4);
      expect(service.writes.single.encoding, native.WorkspaceTextEncoding.utf8);
      expect(registry.isDirty('tab-1'), isFalse);
    });

    test('updates cached document paths after a folder move', () {
      final registry = EditorSessionRegistry();
      final document = registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'src/main.dart',
        );

      registry.updateDocumentPathsAfterMove(
        workspacePath: '/repo/alera',
        oldRelativePath: 'src',
        newRelativePath: 'lib/src',
      );

      expect(document.relativePath, 'lib/src/main.dart');
    });

    test('keeps reveal pending when live session has no reveal callback', () {
      final registry = EditorSessionRegistry();
      const target = WorkspaceEditorRevealTarget(
        line: 3,
        column: 5,
        matchLength: 2,
      );
      registry.register(
        'tab-1',
        EditorSessionHandle(
          isDirty: () => false,
          save: () async {},
          discard: () async {},
        ),
      );

      registry.reveal('tab-1', target);

      expect(registry.takePendingReveal('tab-1'), target);
    });

    test(
      'external watcher reloads only clean documents with a changed disk token',
      () async {
        final registry = EditorSessionRegistry();
        addTearDown(registry.dispose);
        final service = _FakeWorkspaceFileService()
          ..contentTokens['note.txt'] = 'token-1';
        final document = registry.documentFor('tab-1')
          ..attachFile(workspacePath: '/repo/alera', relativePath: 'note.txt')
          ..acceptLoaded(
            _editorFile(
              rawContent: 'original',
              displayContent: 'original',
              contentToken: 'token-1',
            ),
          );
        var reloads = 0;
        registry.register(
          'tab-1',
          EditorSessionHandle(
            isDirty: () => document.isDirty,
            save: () async {},
            discard: () async {},
            reload: () async => reloads += 1,
          ),
        );

        await registry.reloadExternallyChangedCleanFiles(
          workspaceFiles: service,
          workspacePath: '/repo/alera',
          relativePaths: const <String>['note.txt'],
        );
        expect(reloads, 0, reason: 'Alera own-save events must be ignored');

        service.contentTokens['note.txt'] = 'token-2';
        await registry.reloadExternallyChangedCleanFiles(
          workspaceFiles: service,
          workspacePath: '/repo/alera',
          relativePaths: const <String>['note.txt'],
        );
        expect(reloads, 1);

        document.updateCurrentText('local edit');
        service.contentTokens['note.txt'] = 'token-3';
        final readsBeforeDirtyEvent = service.contentTokenReads.length;
        await registry.reloadExternallyChangedCleanFiles(
          workspaceFiles: service,
          workspacePath: '/repo/alera',
          relativePaths: const <String>['note.txt'],
        );
        expect(reloads, 1, reason: 'dirty editor content must not be replaced');
        expect(service.contentTokenReads, hasLength(readsBeforeDirtyEvent));
      },
    );

    test('clears clean snapshot when live session has no reload callback', () {
      final registry = EditorSessionRegistry();
      final document = registry.documentFor('tab-1')
        ..attachFile(workspacePath: '/repo/alera', relativePath: 'note.txt')
        ..acceptLoaded(
          _editorFile(rawContent: 'original', displayContent: 'original'),
        );
      registry.register(
        'tab-1',
        EditorSessionHandle(
          isDirty: () => false,
          save: () async {},
          discard: () async {},
        ),
      );

      registry.reloadCleanFiles(
        workspacePath: '/repo/alera',
        relativePaths: const <String>['note.txt'],
      );

      expect(document.hasSnapshot, isFalse);
    });
  });
}

class _FakeWorkspaceFileService extends WorkspaceFileService {
  final List<_EditorWrite> writes = <_EditorWrite>[];
  final Map<String, String?> contentTokens = <String, String?>{};
  final List<String> contentTokenReads = <String>[];

  @override
  Future<String?> contentTokenForFile({
    required String workspacePath,
    required String relativePath,
  }) async {
    contentTokenReads.add(relativePath);
    return contentTokens[relativePath];
  }

  @override
  Future<native.WorkspaceEditorTextFile> writeEditorTextFile({
    required String workspacePath,
    required String relativePath,
    required String currentDisplayContent,
    required String? originalRawContent,
    required String? originalDisplayContent,
    required String? expectedContentToken,
    required bool overwriteIfChanged,
    required int tabSize,
    required native.WorkspaceTextEncoding encoding,
  }) async {
    writes.add(
      _EditorWrite(
        relativePath: relativePath,
        currentDisplayContent: currentDisplayContent,
        originalRawContent: originalRawContent,
        originalDisplayContent: originalDisplayContent,
        tabSize: tabSize,
        encoding: encoding,
      ),
    );
    return native.WorkspaceEditorTextFile(
      rawContent: currentDisplayContent,
      displayContent: currentDisplayContent,
      contentToken: '$relativePath-saved',
      modifiedMillis: 1,
      size: .from(currentDisplayContent.length),
      encoding: encoding,
    );
  }
}

native.WorkspaceFileEntry _workspaceEntry(
  String relativePath, {
  native.WorkspaceFileGitStatus? gitStatus,
  bool isIgnored = false,
  bool isHidden = false,
  bool isSymlink = false,
  bool isProtected = false,
  bool hasChildrenHint = false,
}) {
  return native.WorkspaceFileEntry(
    relativePath: relativePath,
    name: relativePath.split('/').last,
    kind: native.WorkspaceFileKind.file,
    size: .from(42),
    modifiedMillis: 7,
    contentToken: '$relativePath-token',
    isIgnored: isIgnored,
    isHidden: isHidden,
    isSymlink: isSymlink,
    isProtected: isProtected,
    hasChildrenHint: hasChildrenHint,
    gitStatus: gitStatus,
  );
}

native.WorkspaceEditorTextFile _editorFile({
  required String rawContent,
  required String displayContent,
  String contentToken = 'token-1',
  native.WorkspaceTextEncoding encoding = native.WorkspaceTextEncoding.utf8,
}) {
  return native.WorkspaceEditorTextFile(
    rawContent: rawContent,
    displayContent: displayContent,
    contentToken: contentToken,
    modifiedMillis: 0,
    size: .from(rawContent.length),
    encoding: encoding,
  );
}

class const _EditorWrite({
  required final String relativePath,
  required final String currentDisplayContent,
  required final String? originalRawContent,
  required final String? originalDisplayContent,
  required final int tabSize,
  required final native.WorkspaceTextEncoding encoding,
});
