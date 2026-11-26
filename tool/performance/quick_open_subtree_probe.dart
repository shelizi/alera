// ignore_for_file: avoid_print, implementation_imports

/// Exercise the actual OS watcher -> native journal -> generated Dart API.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/rust/api/workspace_files.dart' as api;
import 'package:alera/src/rust/frb_generated.dart' as rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

void require(bool value, String message) {
  if (!value) throw StateError(message);
}

Future<void> main(List<String> args) async {
  if (args.length != 1)
    throw ArgumentError('Supply the freshly built native library path');
  await rust.RustLib.init(externalLibrary: ExternalLibrary.open(args.single));
  final root = Directory.systemTemp.createTempSync('alera-quick-open-delta-');
  final sessions = <api.WorkspaceQuickOpenSession>[];
  api.WorkspaceExplorerWatcherHandle? watcher;
  StreamIterator<api.WorkspaceExplorerWatchBatch>? events;
  try {
    for (final folder in ['src/deep', 'lib/nested', 'other']) {
      Directory('${root.path}/$folder').createSync(recursive: true);
      File('${root.path}/$folder/original.rs').writeAsStringSync('original');
    }
    File('${root.path}/.gitignore').writeAsStringSync('*.tmp\n');
    watcher = await api.startWorkspaceExplorerWatcher(workspacePath: root.path);
    events = StreamIterator(api.watchWorkspaceExplorerEvents(handle: watcher));
    Future<api.WorkspaceQuickOpenSession> full() async {
      final session = await api.startWorkspaceQuickOpenSession(
        workspacePath: root.path,
        excludedDirectories: const [],
      );
      sessions.add(session);
      return session;
    }

    Future<api.WorkspaceQuickOpenRefresh> refresh(
      api.WorkspaceQuickOpenSession previous,
    ) async {
      final result = await api.refreshWorkspaceQuickOpenSession(
        workspacePath: root.path,
        excludedDirectories: const [],
        previous: previous,
      );
      sessions.add(result.session);
      return result;
    }

    Future<String> fingerprint(api.WorkspaceQuickOpenSession session) async {
      final values = <Object>[];
      for (final query in ['', 'original', 'new', 'moved', 'tmp', 'other']) {
        for (final ignored in [false, true]) {
          final matches = await api.searchWorkspaceExplorerFilterSession(
            session: session,
            query: query,
            limit: 100,
            includeGitignored: ignored,
          );
          values.add([
            for (final match in matches)
              [match.relativePath, match.score, match.isDirectory],
          ]);
        }
      }
      return jsonEncode(values);
    }

    Future<void> observe(Future<bool> pending, Set<String> paths) async {
      final seen = <String>{};
      var next = pending;
      while (await next.timeout(const Duration(seconds: 10))) {
        seen.addAll(events!.current.changedRelativePaths);
        if (seen.containsAll(paths)) {
          // Native journal recording precedes transport. Allow trailing OS
          // notifications to settle, without needing another stream batch.
          await Future<void>.delayed(const Duration(milliseconds: 150));
          return;
        }
        next = events!.moveNext();
      }
      throw StateError('watch stream ended before $paths');
    }

    final firstEvent = events.moveNext();
    await Future<void>.delayed(const Duration(milliseconds: 200));
    final initial = await full();
    final initialFingerprint = await fingerprint(initial);
    File('${root.path}/src/deep/new.rs').writeAsStringSync('new');
    File('${root.path}/lib/nested/new.tmp').writeAsStringSync('ignored');
    await observe(firstEvent, {'src/deep/new.rs', 'lib/nested/new.tmp'});
    final delta = await refresh(initial);
    require(
      delta.incremental,
      'Localized OS notifications did not use incremental discovery',
    );
    final reference = await full();
    require(
      await fingerprint(delta.session) == await fingerprint(reference),
      'Delta differs from full snapshot',
    );
    require(
      await fingerprint(initial) == initialFingerprint,
      'Original index was mutated',
    );

    final nextEvent = events.moveNext();
    File('${root.path}/src/deep/new.rs')
        .renameSync('${root.path}/lib/nested/moved.rs');
    File('${root.path}/src/deep/original.rs').deleteSync();
    await observe(nextEvent, {
      'src/deep/new.rs',
      'lib/nested/moved.rs',
      'src/deep/original.rs',
    });
    final moved = await refresh(delta.session);
    require(
      moved.incremental,
      'Localized rename/delete did not use incremental discovery',
    );
    require(
      await fingerprint(moved.session) == await fingerprint(await full()),
      'Rename/delete delta differs',
    );

    final policyEvent = events.moveNext();
    File('${root.path}/.gitignore').writeAsStringSync('other/\n');
    await observe(policyEvent, {'.gitignore'});
    final policy = await refresh(moved.session);
    require(
      !policy.incremental,
      'Ignore-rule change must use complete discovery',
    );
    require(
      await fingerprint(policy.session) == await fingerprint(await full()),
      'Policy result differs',
    );
    print(
      jsonEncode({
        'localized_create': true,
        'cross_directory_rename_delete': true,
        'ignore_rule_full_fallback': true,
        'old_snapshot_immutable': true,
        'exact_full_equivalence': true,
      }),
    );
  } finally {
    if (watcher != null)
      await api.stopWorkspaceExplorerWatcher(handle: watcher);
    if (events != null) {
      try {
        await events.cancel().timeout(const Duration(seconds: 3));
      } catch (_) {}
    }
    for (final session in sessions) {
      await api.stopWorkspaceQuickOpenSession(session: session);
    }
    root.deleteSync(recursive: true);
  }
}
