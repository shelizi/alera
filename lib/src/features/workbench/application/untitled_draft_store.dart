import 'dart:async';
import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

/// Durable scratch documents. App support (not the OS temp directory) keeps
/// untitled tabs recoverable after a normal exit or unexpected termination.
class UntitledDraftStore {
  static final UntitledDraftStore shared = UntitledDraftStore();

  UntitledDraftStore({Future<Directory> Function()? supportDirectory})
    : _supportDirectory = supportDirectory ?? getApplicationSupportDirectory;

  final Future<Directory> Function() _supportDirectory;
  final Map<String, Future<void>> _pending = <String, Future<void>>{};
  final Set<String> _deletedTabs = <String>{};

  Future<String?> read(String tabId) async {
    _checkTabId(tabId);
    final pending = _pending[tabId];
    if (pending != null) await pending;
    final directory = await _directory();
    final path = p.join(directory.path, '$tabId.txt');
    for (final candidate in <File>[
      File(path),
      File('$path.tmp'),
      File('$path.bak'),
    ]) {
      if (await candidate.exists()) {
        return candidate.readAsString();
      }
    }
    return null;
  }

  Future<void> write(String tabId, String contents) {
    _checkTabId(tabId);
    if (_deletedTabs.contains(tabId)) return Future<void>.value();
    return _serialize(tabId, () async {
      if (_deletedTabs.contains(tabId)) return;
      final directory = await _directory();
      await directory.create(recursive: true);
      final path = p.join(directory.path, '$tabId.txt');
      final temp = File('$path.tmp');
      await temp.writeAsString(contents, flush: true);
      final current = File(path);
      final backup = File('$path.bak');
      if (await current.exists()) {
        if (await backup.exists()) await backup.delete();
        await current.rename(backup.path);
      }
      await temp.rename(path);
    });
  }

  Future<void> delete(String tabId) {
    _checkTabId(tabId);
    _deletedTabs.add(tabId);
    return _serialize(tabId, () async {
      final directory = await _directory();
      final path = p.join(directory.path, '$tabId.txt');
      for (final file in <File>[
        File(path),
        File('$path.tmp'),
        File('$path.bak'),
      ]) {
        if (await file.exists()) await file.delete();
      }
    });
  }

  Future<Directory> _directory() async {
    final support = await _supportDirectory();
    return Directory(p.join(support.path, 'untitled-drafts'));
  }

  Future<T> _serialize<T>(String tabId, Future<T> Function() action) {
    final previous = _pending[tabId] ?? Future<void>.value();
    final result = previous.then((_) => action());
    final barrier = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace __) {},
    );
    _pending[tabId] = barrier;
    unawaited(
      barrier.then((_) {
        if (identical(_pending[tabId], barrier)) _pending.remove(tabId);
      }),
    );
    return result;
  }

  void _checkTabId(String tabId) {
    if (!RegExp(r'^[a-zA-Z0-9_-]+$').hasMatch(tabId)) {
      throw ArgumentError.value(tabId, 'tabId', 'Invalid draft ID');
    }
  }
}
