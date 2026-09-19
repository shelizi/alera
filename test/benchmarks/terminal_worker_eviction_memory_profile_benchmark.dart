import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

const _modeName = String.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_MODE',
  defaultValue: 'hard',
);
const _workerCount = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS',
  defaultValue: 4,
);
const _snapshotRows = int.fromEnvironment(
  'ALERA_TERMINAL_EVICTION_PROFILE_ROWS',
  defaultValue: 1000,
);
const _cols = 160;
const _rows = 44;
const _maxLines = 12_000;
const _revealRuns = 5;
const _marker = 'P4-WORKER-END';

void main() {
  test('T8e P4 parser-worker eviction profile ($_modeName)', () async {
    expect(_modeName, anyOf('soft', 'hard'));
    expect(_workerCount, greaterThan(1));
    expect(_snapshotRows, greaterThan(0));
    final hardEviction = _modeName == 'hard';

    final warmup = await _startWorker();
    await warmup.parseHidden('warmup\r\n');
    await warmup.close();
    await Future<void>.delayed(const Duration(milliseconds: 250));

    final snapshotText = _buildSnapshotText(
      rows: _snapshotRows,
      marker: _marker,
    );
    final snapshotBytes = utf8.encode(snapshotText).length;
    expect(snapshotBytes, _snapshotRows * 256);

    final rssBaseline = ProcessInfo.currentRss;
    final workers = <TerminalXtermWorker>[];
    for (var index = 0; index < _workerCount; index++) {
      final worker = await _startWorker();
      await worker.parseHidden(snapshotText);
      workers.add(worker);
    }
    await Future<void>.delayed(const Duration(milliseconds: 250));
    final rssHydrated = ProcessInfo.currentRss;

    final retainedStates = <TerminalXtermWorkerRetainedState>[];
    if (hardEviction) {
      for (final worker in workers) {
        final exported = await worker.exportRetainedState();
        expect(exported.blockers, isEmpty);
        expect(exported.state, isNotNull);
        retainedStates.add(exported.state!);
      }
      for (final worker in workers) {
        await worker.close();
      }
      workers.clear();
    }

    await Future<void>.delayed(const Duration(milliseconds: 500));
    final rssEvicted = ProcessInfo.currentRss;

    final revealSamples = <_RevealSample>[];
    for (var run = 0; run < _revealRuns; run++) {
      TerminalXtermWorker? restoredWorker;
      final watch = Stopwatch()..start();
      final worker = hardEviction
          ? restoredWorker = await _startWorker(
              retainedState: retainedStates.first,
            )
          : workers.first;
      final profile = await worker.profileSnapshotBufferDelta();
      watch.stop();

      expect(profile.delta.fullRepaint, isTrue);
      expect(profile.delta.bufferLength, greaterThan(0));
      expect(profile.delta.rowDeltas, isNotEmpty);
      revealSamples.add(
        _RevealSample(
          totalMicros: watch.elapsedMicroseconds,
          rawRoundtripMicros: profile.rawRoundtripMicros,
          materializeMicros: profile.materializeMicros,
          decodeMicros: profile.decodeMicros,
        ),
      );
      await restoredWorker?.close();
    }

    for (final worker in workers) {
      await worker.close();
    }

    final hydratedDelta = rssHydrated - rssBaseline;
    final evictedDelta = rssEvicted - rssBaseline;
    final reclaimed = rssHydrated - rssEvicted;
    final report = <String, Object?>{
      'version': 1,
      'surface': 'flutter_tester_direct_worker',
      'mode': _modeName,
      'workers': _workerCount,
      'snapshot_rows_per_worker': _snapshotRows,
      'snapshot_bytes_per_worker': snapshotBytes,
      'rss_baseline_bytes': rssBaseline,
      'rss_hydrated_bytes': rssHydrated,
      'rss_evicted_bytes': rssEvicted,
      'rss_hydrated_delta_bytes': hydratedDelta,
      'rss_evicted_delta_bytes': evictedDelta,
      'rss_reclaimed_after_eviction_bytes': reclaimed,
      'rss_reclaimed_fraction_of_hydrated_delta': hydratedDelta > 0
          ? reclaimed / hydratedDelta
          : null,
      'hard_evicted_workers': hardEviction ? _workerCount : 0,
      'reveal_total_ms_samples': [
        for (final sample in revealSamples) sample.totalMicros / 1000,
      ],
      'reveal_total_median_ms':
          _medianMicros(revealSamples.map((sample) => sample.totalMicros)) /
          1000,
      'reveal_total_p95_ms':
          _percentileMicros(
            revealSamples.map((sample) => sample.totalMicros),
            0.95,
          ) /
          1000,
      'reveal_raw_roundtrip_median_ms':
          _medianMicros(
            revealSamples.map((sample) => sample.rawRoundtripMicros),
          ) /
          1000,
      'reveal_materialize_median_ms':
          _medianMicros(
            revealSamples.map((sample) => sample.materializeMicros),
          ) /
          1000,
      'reveal_decode_median_ms':
          _medianMicros(revealSamples.map((sample) => sample.decodeMicros)) /
          1000,
    };

    // ignore: avoid_print
    print('T8E_P4_WORKER_PROFILE ${jsonEncode(report)}');
  }, timeout: const Timeout(Duration(minutes: 3)));
}

Future<TerminalXtermWorker> _startWorker({
  TerminalXtermWorkerRetainedState? retainedState,
}) {
  return TerminalXtermWorker.start(
    cols: _cols,
    rows: _rows,
    maxLines: _maxLines,
    retainedState: retainedState,
  );
}

String _buildSnapshotText({required int rows, required String marker}) {
  const styledToken = '\x1b[32mOK\x1b[0m';
  final body = List<String>.filled(21, styledToken).join();
  return List<String>.generate(rows, (index) {
    final suffix = index == rows - 1
        ? marker
        : 'ROW-${index.toString().padLeft(5, '0')}';
    return '$body${suffix.padRight(23, '.')}\r\n';
  }).join();
}

double _medianMicros(Iterable<int> values) {
  final sorted = values.toList()..sort();
  final middle = sorted.length ~/ 2;
  if (sorted.length.isOdd) {
    return sorted[middle].toDouble();
  }
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

double _percentileMicros(Iterable<int> values, double percentile) {
  final sorted = values.toList()..sort();
  final index = (sorted.length * percentile).ceil() - 1;
  return sorted[index.clamp(0, sorted.length - 1)].toDouble();
}

final class _RevealSample {
  const _RevealSample({
    required this.totalMicros,
    required this.rawRoundtripMicros,
    required this.materializeMicros,
    required this.decodeMicros,
  });

  final int totalMicros;
  final int rawRoundtripMicros;
  final int materializeMicros;
  final int decodeMicros;
}
