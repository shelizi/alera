import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_editor_file_watcher.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('watches only parent directories of open editor files', () async {
    final service = _WatcherFileService();
    final registry = EditorSessionRegistry();
    addTearDown(registry.dispose);
    final watcher = WorkspaceEditorFileWatcher(
      workspaceFiles: service,
      editorSessions: registry,
    );
    addTearDown(watcher.dispose);

    await watcher.update(
      workspacePath: '/repo/alera',
      openRelativePaths: const <String>[
        'lib/src/main.dart',
        'lib/src/other.dart',
        'README.md',
      ],
    );

    expect(service.starts, <String>['/repo/alera']);
    expect(service.watchedDirectories.single.toSet(), <String>{'lib/src', ''});

    await watcher.update(
      workspacePath: '/repo/alera',
      openRelativePaths: const <String>['README.md', 'lib/src/main.dart'],
    );
    expect(service.watchedDirectories, hasLength(1));
  });

  test(
    'changed events reload a clean editor but preserve a dirty editor',
    () async {
      final service = _WatcherFileService()
        ..contentTokens['lib/main.dart'] = 'token-1';
      final registry = EditorSessionRegistry();
      addTearDown(registry.dispose);
      final document = registry.documentFor('tab-1')
        ..attachFile(
          workspacePath: '/repo/alera',
          relativePath: 'lib/main.dart',
        )
        ..acceptLoaded(_editorFile('original', token: 'token-1'));
      var reloads = 0;
      registry.register(
        'tab-1',
        EditorSessionHandle(
          isDirty: () => document.isDirty,
          save: () async {},
          discard: () async {},
          reload: () async {
            reloads += 1;
            document.acceptLoaded(
              _editorFile(
                'external',
                token: service.contentTokens['lib/main.dart']!,
              ),
            );
          },
        ),
      );
      final watcher = WorkspaceEditorFileWatcher(
        workspaceFiles: service,
        editorSessions: registry,
      );
      addTearDown(watcher.dispose);
      await watcher.update(
        workspacePath: '/repo/alera',
        openRelativePaths: const <String>['lib/main.dart'],
      );

      service.emitChanged('lib/main.dart');
      await _flush();
      expect(reloads, 0, reason: 'same token includes Alera own-save events');

      service.contentTokens['lib/main.dart'] = 'token-2';
      service.emitChanged('lib/main.dart');
      await _flush();
      expect(reloads, 1);
      expect(document.contentToken, 'token-2');

      document.updateCurrentText('local edit');
      final readsBefore = service.contentTokenReads.length;
      service.contentTokens['lib/main.dart'] = 'token-3';
      service.emitChanged('lib/main.dart');
      await _flush();
      expect(reloads, 1);
      expect(service.contentTokenReads, hasLength(readsBefore));
      expect(document.currentText, 'local edit');
    },
  );

  test(
    'root rescan checks all clean open files and notifies only this workspace',
    () async {
      const workspace = '/repo/alera';
      final service = _WatcherFileService()
        ..contentTokens['src/clean.dart'] = 'fixture-new-clean'
        ..contentTokens['src/dirty.dart'] = 'fixture-new-dirty'
        ..contentTokens['src/other.dart'] = 'fixture-new-other';
      final registry = EditorSessionRegistry();
      addTearDown(registry.dispose);
      final clean = registry.documentFor('clean')
        ..attachFile(workspacePath: workspace, relativePath: 'src/clean.dart')
        ..acceptLoaded(_editorFile('old clean', token: 'fixture-old-clean'));
      final dirty = registry.documentFor('dirty')
        ..attachFile(workspacePath: workspace, relativePath: 'src/dirty.dart')
        ..acceptLoaded(_editorFile('old dirty', token: 'fixture-old-dirty'))
        ..updateCurrentText('unsaved local edit');
      final other = registry.documentFor('other')
        ..attachFile(
          workspacePath: '/repo/other',
          relativePath: 'src/other.dart',
        )
        ..acceptLoaded(
          _editorFile('other workspace', token: 'fixture-old-other'),
        );
      var reloads = 0;
      registry.register(
        'clean',
        EditorSessionHandle(
          isDirty: () => clean.isDirty,
          save: () async {},
          discard: () async {},
          reload: () async {
            reloads++;
            clean.acceptLoaded(
              _editorFile('external clean', token: 'fixture-new-clean'),
            );
          },
        ),
      );
      final localViewer = registry.documentChangesForPath(
        workspacePath: workspace,
        relativePath: 'docs/viewer.md',
      );
      final otherViewer = registry.documentChangesForPath(
        workspacePath: '/repo/other',
        relativePath: 'docs/viewer.md',
      );
      var localNotifications = 0;
      var otherNotifications = 0;
      void localChanged() => localNotifications++;
      void otherChanged() => otherNotifications++;
      localViewer.addListener(localChanged);
      otherViewer.addListener(otherChanged);
      addTearDown(() {
        localViewer.removeListener(localChanged);
        otherViewer.removeListener(otherChanged);
      });
      final watcher = WorkspaceEditorFileWatcher(
        workspaceFiles: service,
        editorSessions: registry,
      );
      addTearDown(watcher.dispose);
      await watcher.update(
        workspacePath: workspace,
        openRelativePaths: [
          'src/clean.dart',
          'src/dirty.dart',
          'docs/viewer.md',
        ],
      );

      // No concrete file paths survive an overflowing native mailbox.
      service.emitChanged('');
      await _flush();
      expect(reloads, 1);
      expect(clean.currentText, 'external clean');
      expect(dirty.currentText, 'unsaved local edit');
      expect(other.currentText, 'other workspace');
      expect(service.contentTokenReads, ['src/clean.dart']);
      expect(localNotifications, 1);
      expect(otherNotifications, 0);
      service.emitChanged('');
      await _flush();
      expect(reloads, 1, reason: 'unchanged disk token preserves undo state');
    },
  );

  test(
    'editor watcher bounds accumulated paths and recovers with a root pass',
    () async {
      const workspace = '/repo/alera';
      final service = _WatcherFileService()
        ..contentTokens['busy.txt'] = 'busy-original'
        ..contentTokens['target.txt'] = 'target-original'
        ..contentTokens['dirty.txt'] = 'dirty-external'
        ..contentTokens['other.txt'] = 'other-external';
      final registry = _RecordingEditorRegistry();
      addTearDown(registry.dispose);
      registry.documentFor('busy')
        ..attachFile(workspacePath: workspace, relativePath: 'busy.txt')
        ..acceptLoaded(_editorFile('busy', token: 'busy-original'));
      final target = registry.documentFor('target')
        ..attachFile(workspacePath: workspace, relativePath: 'target.txt')
        ..acceptLoaded(_editorFile('target', token: 'target-original'));
      final dirty = registry.documentFor('dirty')
        ..attachFile(workspacePath: workspace, relativePath: 'dirty.txt')
        ..acceptLoaded(_editorFile('dirty', token: 'dirty-original'))
        ..updateCurrentText('unsaved');
      final other = registry.documentFor('other')
        ..attachFile(workspacePath: '/repo/other', relativePath: 'other.txt')
        ..acceptLoaded(_editorFile('other', token: 'other-original'));
      var targetReloads = 0;
      registry.register(
        'target',
        EditorSessionHandle(
          isDirty: () => target.isDirty,
          save: () async {},
          discard: () async {},
          reload: () async {
            targetReloads++;
            target.acceptLoaded(
              _editorFile('external target', token: 'target-external'),
            );
          },
        ),
      );
      final viewer = registry.documentChangesForPath(
        workspacePath: workspace,
        relativePath: 'viewer.md',
      );
      final otherViewer = registry.documentChangesForPath(
        workspacePath: '/repo/other',
        relativePath: 'viewer.md',
      );
      var viewerNotifications = 0;
      var otherNotifications = 0;
      void viewerChanged() => viewerNotifications++;
      void otherChanged() => otherNotifications++;
      viewer.addListener(viewerChanged);
      otherViewer.addListener(otherChanged);
      addTearDown(() {
        viewer.removeListener(viewerChanged);
        otherViewer.removeListener(otherChanged);
      });
      final watcher = WorkspaceEditorFileWatcher(
        workspaceFiles: service,
        editorSessions: registry,
      );
      addTearDown(watcher.dispose);
      await watcher.update(
        workspacePath: workspace,
        openRelativePaths: ['busy.txt', 'target.txt', 'dirty.txt', 'viewer.md'],
      );

      Future<Completer<void>> holdReload() async {
        final started = Completer<void>();
        final release = Completer<void>();
        addTearDown(() {
          if (!release.isCompleted) release.complete();
        });
        service.beforeContentTokenRead = (_, relativePath) async {
          if (relativePath == 'busy.txt' && !started.isCompleted) {
            started.complete();
            await release.future;
          }
        };
        service.emitChanged('busy.txt');
        await started.future;
        return release;
      }

      // The exact budget keeps detail, even when the same paths repeat.
      final first = await holdReload();
      final exact = List.generate(512, (i) => 'generated/file-$i.txt');
      service.emitBatch(exact);
      service.emitBatch(exact.reversed.toList());
      await _flush();
      first.complete();
      await _flush();
      expect(registry.reloadBatches.map((batch) => batch.paths).toList(), [
        ['busy.txt'],
        exact,
      ]);
      expect(viewerNotifications, 0);

      // Multiple individually small native batches can accumulate during IO.
      // 10,000 distinct paths must collapse to one root sentinel, not remain
      // queued as 10,000 paths or cause 10,000 follow-up metadata lookups.
      registry.reloadBatches.clear();
      service.contentTokenReads.clear();
      final second = await holdReload();
      service.contentTokens['target.txt'] = 'target-external';
      for (var batch = 0; batch < 100; batch++) {
        service.emitBatch(
          List.generate(100, (i) => 'generated/storm-${batch * 100 + i}.txt'),
        );
      }
      service.emitChanged('target.txt');
      await _flush();
      second.complete();
      await _flush();

      expect(registry.reloadBatches.map((batch) => batch.paths).toList(), [
        ['busy.txt'],
        [''],
      ]);
      expect(service.contentTokenReads, ['busy.txt', 'busy.txt', 'target.txt']);
      expect(targetReloads, 1);
      expect(target.currentText, 'external target');
      expect(dirty.currentText, 'unsaved');
      expect(other.currentText, 'other');
      expect(viewerNotifications, 1);
      expect(otherNotifications, 0);

      // After draining the rescan, normal precise notifications resume.
      registry.reloadBatches.clear();
      service.emitChanged('viewer.md');
      await _flush();
      expect(registry.reloadBatches.single.paths, ['viewer.md']);
      expect(viewerNotifications, 2);
      expect(targetReloads, 1);
    },
  );

  test('native root sentinel dominates pending detail and workspace switch clears it', () async {
    final service = _WatcherFileService()
      ..contentTokens['busy.txt'] = 'unchanged';
    final registry = _RecordingEditorRegistry();
    addTearDown(registry.dispose);
    registry.documentFor('busy')
      ..attachFile(workspacePath: '/repo/one', relativePath: 'busy.txt')
      ..acceptLoaded(_editorFile('busy', token: 'unchanged'));
    final watcher = WorkspaceEditorFileWatcher(
      workspaceFiles: service,
      editorSessions: registry,
    );
    addTearDown(watcher.dispose);
    await watcher.update(
      workspacePath: '/repo/one',
      openRelativePaths: ['busy.txt'],
    );
    final started = Completer<void>();
    final release = Completer<void>();
    addTearDown(() {
      if (!release.isCompleted) release.complete();
    });
    service.beforeContentTokenRead = (_, relativePath) async {
      if (relativePath == 'busy.txt' && !started.isCompleted) {
        started.complete();
        await release.future;
      }
    };
    service.emitChanged('busy.txt');
    await started.future;
    service.emitBatch(['first.txt', '', 'later.txt']);
    service.emitChanged('another.txt');
    await _flush();
    release.complete();
    await _flush();
    expect(registry.reloadBatches.map((batch) => batch.paths).toList(), [
      ['busy.txt'],
      [''],
    ]);

    // Overflow queued for the old watcher must never be passed to the new
    // workspace, including while the old metadata query is still in flight.
    registry.reloadBatches.clear();
    final secondStarted = Completer<void>();
    final secondRelease = Completer<void>();
    addTearDown(() {
      if (!secondRelease.isCompleted) secondRelease.complete();
    });
    service.beforeContentTokenRead = (_, relativePath) async {
      if (relativePath == 'busy.txt' && !secondStarted.isCompleted) {
        secondStarted.complete();
        await secondRelease.future;
      }
    };
    service.emitChanged('busy.txt');
    await secondStarted.future;
    service.emitBatch(List.generate(600, (i) => 'old-$i.txt'));
    await _flush();
    await watcher.update(
      workspacePath: '/repo/two',
      openRelativePaths: ['new.txt'],
    );
    service.emitChanged('new.txt');
    await _flush();
    secondRelease.complete();
    await _flush();
    expect(registry.reloadBatches.map((batch) => batch.workspace).toList(), [
      '/repo/one',
      '/repo/two',
    ]);
    expect(registry.reloadBatches.map((batch) => batch.paths).toList(), [
      ['busy.txt'],
      ['new.txt'],
    ]);
  });

  test(
    'registry tolerates closing or replacing tabs during async metadata lookup',
    () async {
      const workspace = '/repo/alera';
      final service = _WatcherFileService()
        ..contentTokens['closing.txt'] = 'closing-disk'
        ..contentTokens['surviving.txt'] = 'surviving-disk'
        ..contentTokens['dirty.txt'] = 'dirty-disk';
      final registry = EditorSessionRegistry();
      addTearDown(registry.dispose);
      final closing = registry.documentFor('closing')
        ..attachFile(workspacePath: workspace, relativePath: 'closing.txt')
        ..acceptLoaded(_editorFile('closing', token: 'closing-old'));
      final surviving = registry.documentFor('surviving')
        ..attachFile(workspacePath: workspace, relativePath: 'surviving.txt')
        ..acceptLoaded(_editorFile('surviving', token: 'surviving-old'));
      final dirty = registry.documentFor('dirty')
        ..attachFile(workspacePath: workspace, relativePath: 'dirty.txt')
        ..acceptLoaded(_editorFile('dirty', token: 'dirty-old'));
      var replacementReloads = 0;
      var survivingReloads = 0;
      registry.register(
        'surviving',
        EditorSessionHandle(
          isDirty: () => surviving.isDirty,
          save: () async {},
          discard: () async {},
          reload: () async {
            survivingReloads++;
            surviving.acceptLoaded(
              _editorFile('external surviving', token: 'surviving-disk'),
            );
          },
        ),
      );
      final started = Completer<void>();
      final release = Completer<void>();
      addTearDown(() {
        if (!release.isCompleted) release.complete();
      });
      service.beforeContentTokenRead = (_, relativePath) async {
        if (relativePath == 'closing.txt') {
          started.complete();
          await release.future;
        } else if (relativePath == 'dirty.txt') {
          // Simulate an edit during this document's metadata lookup.
          dirty.updateCurrentText('unsaved during lookup');
        }
      };
      final refresh = registry.reloadExternallyChangedCleanFiles(
        workspaceFiles: service,
        workspacePath: workspace,
        relativePaths: [''],
      );
      await started.future;
      registry.forget('closing');
      final replacement = registry.documentFor('closing')
        ..attachFile(workspacePath: workspace, relativePath: 'closing.txt')
        ..acceptLoaded(
          _editorFile('new session', token: 'replacement-original'),
        );
      registry.register(
        'closing',
        EditorSessionHandle(
          isDirty: () => replacement.isDirty,
          save: () async {},
          discard: () async {},
          reload: () async {
            replacementReloads++;
          },
        ),
      );
      registry.documentFor('new-tab')
        ..attachFile(workspacePath: workspace, relativePath: 'new.txt')
        ..acceptLoaded(_editorFile('new tab', token: 'new-original'));
      release.complete();
      await refresh;
      expect(closing.currentText, 'closing');
      expect(replacement.currentText, 'new session');
      expect(replacementReloads, 0);
      expect(survivingReloads, 1);
      expect(surviving.currentText, 'external surviving');
      expect(dirty.currentText, 'unsaved during lookup');
      expect(service.contentTokenReads, [
        'closing.txt',
        'surviving.txt',
        'dirty.txt',
      ]);
    },
  );

  test('changed events notify path listeners without an editor tab', () async {
    final service = _WatcherFileService();
    final registry = EditorSessionRegistry();
    addTearDown(registry.dispose);
    final changes = registry.documentChangesForPath(
      workspacePath: '/repo/alera',
      relativePath: 'docs/readme.md',
    );
    var notifications = 0;
    void handleChange() => notifications += 1;
    changes.addListener(handleChange);
    addTearDown(() => changes.removeListener(handleChange));

    final watcher = WorkspaceEditorFileWatcher(
      workspaceFiles: service,
      editorSessions: registry,
    );
    addTearDown(watcher.dispose);
    await watcher.update(
      workspacePath: '/repo/alera',
      openRelativePaths: const <String>['docs/readme.md'],
    );

    service.emitChanged('docs/readme.md');
    await _flush();

    expect(notifications, 1);
  });

  test(
    'switching workspace stops the old watcher before starting the new one',
    () async {
      final service = _WatcherFileService();
      final registry = EditorSessionRegistry();
      addTearDown(registry.dispose);
      final watcher = WorkspaceEditorFileWatcher(
        workspaceFiles: service,
        editorSessions: registry,
      );
      addTearDown(watcher.dispose);

      await watcher.update(
        workspacePath: '/repo/one',
        openRelativePaths: const <String>['a.txt'],
      );
      await watcher.update(
        workspacePath: '/repo/two',
        openRelativePaths: const <String>['b.txt'],
      );

      expect(service.starts, <String>['/repo/one', '/repo/two']);
      expect(service.stops, contains('watcher-1'));
    },
  );
}

Future<void> _flush() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

native.WorkspaceEditorTextFile _editorFile(
  String text, {
  required String token,
}) {
  return native.WorkspaceEditorTextFile(
    rawContent: text,
    displayContent: text,
    contentToken: token,
    modifiedMillis: 1,
    size: .from(text.length),
    encoding: native.WorkspaceTextEncoding.utf8,
  );
}

class _RecordingEditorRegistry extends EditorSessionRegistry {
  final reloadBatches = <({String workspace, List<String> paths})>[];

  @override
  Future<void> reloadExternallyChangedCleanFiles({
    required WorkspaceFileService workspaceFiles,
    required String workspacePath,
    required Iterable<String> relativePaths,
  }) {
    final paths = relativePaths.toList(growable: false);
    reloadBatches.add((workspace: workspacePath, paths: paths));
    return super.reloadExternallyChangedCleanFiles(
      workspaceFiles: workspaceFiles,
      workspacePath: workspacePath,
      relativePaths: paths,
    );
  }
}

class _WatcherFileService extends WorkspaceFileService {
  final StreamController<native.WorkspaceExplorerWatchBatch> _events =
      StreamController<native.WorkspaceExplorerWatchBatch>.broadcast();
  final List<String> starts = <String>[];
  final List<String> stops = <String>[];
  final List<List<String>> watchedDirectories = <List<String>>[];
  final Map<String, String?> contentTokens = <String, String?>{};
  final List<String> contentTokenReads = <String>[];
  Future<void> Function(String workspacePath, String relativePath)?
  beforeContentTokenRead;
  var _nextWatcher = 0;

  @override
  Future<native.WorkspaceExplorerWatcherHandle> startExplorerWatcher({
    required String workspacePath,
  }) async {
    starts.add(workspacePath);
    _nextWatcher += 1;
    return native.WorkspaceExplorerWatcherHandle(id: 'watcher-$_nextWatcher');
  }

  @override
  Future<void> updateExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
    required List<String> watchedRelativePaths,
  }) async {
    watchedDirectories.add(List<String>.from(watchedRelativePaths));
  }

  @override
  Stream<native.WorkspaceExplorerWatchBatch> watchExplorerEvents({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) => _events.stream;

  @override
  Future<void> stopExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) async {
    stops.add(handle.id);
  }

  @override
  Future<String?> contentTokenForFile({
    required String workspacePath,
    required String relativePath,
  }) async {
    contentTokenReads.add(relativePath);
    await beforeContentTokenRead?.call(workspacePath, relativePath);
    return contentTokens[relativePath];
  }

  void emitBatch(List<String> paths) {
    _events.add(
      native.WorkspaceExplorerWatchBatch(
        directoryRelativePaths: const [''],
        changedRelativePaths: paths,
        coalescedEventCount: paths.length,
      ),
    );
  }

  void emitChanged(String relativePath) {
    final separator = relativePath.lastIndexOf('/');
    _events.add(
      native.WorkspaceExplorerWatchBatch(
        directoryRelativePaths: <String>[
          separator < 0 ? '' : relativePath.substring(0, separator),
        ],
        changedRelativePaths: <String>[relativePath],
        coalescedEventCount: 1,
      ),
    );
  }
}
