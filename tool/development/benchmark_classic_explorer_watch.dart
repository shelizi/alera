import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workspace_explorer_watch_refresh_queue.dart';
import 'package:fake_async/fake_async.dart';

/// In-memory model of the pre-change Explorer watcher drain: each native
/// event immediately starts a scan unless another scan is still in flight.
class _LegacyImmediateWatcher {
  _LegacyImmediateWatcher(this.refresh);

  final Future<void> Function(List<String>) refresh;
  final Set<String> _pending = <String>{};
  bool _inFlight = false;

  void enqueue(Iterable<String> directories) {
    _pending.addAll(directories);
    if (_inFlight || _pending.isEmpty) return;
    _inFlight = true;
    unawaited(_drain());
  }

  Future<void> _drain() async {
    try {
      while (_pending.isNotEmpty) {
        final batch = _pending.toList(growable: false);
        _pending.clear();
        await refresh(batch);
      }
    } finally {
      _inFlight = false;
    }
  }
}

Map<String, Object> _scenario({
  required String name,
  required int signals,
  required Duration spacing,
}) {
  late final Map<String, Object> result;
  fakeAsync((clock) {
    var legacyReads = 0;
    var coalescedReads = 0;
    var legacyDirectories = 0;
    var coalescedDirectories = 0;

    final legacy = _LegacyImmediateWatcher((batch) async {
      legacyReads++;
      legacyDirectories += batch.length;
    });
    final coalesced = WorkspaceExplorerWatchRefreshQueue(
      refresh: (batch) async {
        coalescedReads++;
        coalescedDirectories += batch.length;
      },
    );
    for (var index = 0; index < signals; index++) {
      // Typical build activity changes multiple paths repeatedly.
      final directories = <String>['src', 'lib', 'src'];
      legacy.enqueue(directories);
      coalesced.enqueue(directories);
      clock.flushMicrotasks();
      if (spacing > Duration.zero) clock.elapse(spacing);
    }
    clock.elapse(const Duration(seconds: 2));
    clock.flushMicrotasks();
    coalesced.dispose();

    result = <String, Object>{
      'scenario': name,
      'signals': signals,
      'spacing_ms': spacing.inMilliseconds,
      'legacy_scan_requests': legacyReads,
      'coalesced_scan_requests': coalescedReads,
      'legacy_directory_reads': legacyDirectories,
      'coalesced_directory_reads': coalescedDirectories,
      'note': 'deterministic synthetic scheduling; no real disk I/O or GUI FPS',
    };
  });
  return result;
}

void main() {
  final results = <Map<String, Object>>[
    _scenario(name: 'burst', signals: 200, spacing: Duration.zero),
    _scenario(
      name: 'continuous-build',
      signals: 300,
      spacing: const Duration(milliseconds: 5),
    ),
  ];
  for (final result in results) {
    stdout.writeln(jsonEncode(result));
  }
}
