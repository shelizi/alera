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

class _WatcherFileService extends WorkspaceFileService {
  final StreamController<native.WorkspaceExplorerWatchBatch> _events =
      StreamController<native.WorkspaceExplorerWatchBatch>.broadcast();
  final List<String> starts = <String>[];
  final List<String> stops = <String>[];
  final List<List<String>> watchedDirectories = <List<String>>[];
  final Map<String, String?> contentTokens = <String, String?>{};
  final List<String> contentTokenReads = <String>[];
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
    return contentTokens[relativePath];
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
