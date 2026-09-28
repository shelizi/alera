/// T5 production terminal worker/delta/replica profiling gate.
///
/// This benchmark intentionally uses the same worker and UI replica classes as
/// the shipped parser-worker path. It does not mutate production code. The
/// runtime/render and restore portions of the gate live in the sibling
/// `terminal_flush_cadence_benchmark.dart` and `terminal_restore_benchmark.dart`.
///
/// Run on the target host:
///
///     flutter test test/benchmarks/terminal_production_worker_profile_benchmark.dart
///
/// Every scenario records five fresh-worker samples. `delta proxy bytes` is a
/// stable logical payload estimate for before/after comparisons; Dart isolate
/// does not expose the serialized wire byte count.
library;

import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

const _cols = 160;
const _rows = 44;
const _maxLines = 12_000;
const _measuredRuns = 5;

void main() {
  test('production terminal worker/delta/replica profile gate', () async {
    final scenarios = <_Scenario>[
      _Scenario(
        name: 'sustained compiler/log output',
        run: (harness) async {
          for (var chunk = 0; chunk < 20; chunk++) {
            await harness.visible(_logLines(chunk * 100, 100, ansi: true));
          }
        },
      ),
      _Scenario(
        name: 'bursty agent output',
        run: (harness) async {
          for (var burst = 0; burst < 8; burst++) {
            await harness.visible(_agentBurst(burst, 80));
          }
        },
      ),
      _Scenario(
        name: 'full-screen TUI repaint',
        setup: (harness) async {
          await harness.visible('\x1b[?1049h\x1b[2J\x1b[H');
          harness.resetMetrics();
        },
        run: (harness) async {
          for (var frame = 0; frame < 12; frame++) {
            await harness.visible(_tuiFrame(frame));
          }
        },
      ),
      _Scenario(
        name: 'synchronized-update bursts',
        run: (harness) async {
          for (var burst = 0; burst < 8; burst++) {
            await harness.visible('\x1b[?2026h${_tuiFrame(burst)}');
            await harness.visible(_tuiFrame(burst + 100));
            await harness.visible('\x1b[?2026l');
          }
        },
      ),
      _Scenario(
        name: 'deep scrollback with ongoing output',
        setup: (harness) async {
          for (var chunk = 0; chunk < 18; chunk++) {
            await harness.visible(_logLines(chunk * 500, 500));
          }
          harness.resetMetrics();
        },
        run: (harness) async {
          for (var chunk = 0; chunk < 12; chunk++) {
            await harness.visible(_logLines(20_000 + chunk * 20, 20));
          }
        },
      ),
      _Scenario(
        name: 'keystroke echo with short scrollback',
        setup: (harness) async {
          await harness.visible(_logLines(0, 100));
          harness.resetMetrics();
        },
        run: (harness) async {
          for (var key = 0; key < 200; key++) {
            await harness.visible(String.fromCharCode(0x61 + key % 26));
          }
        },
      ),
      _Scenario(
        name: 'keystroke echo with full scrollback',
        setup: (harness) async {
          for (var chunk = 0; chunk < 26; chunk++) {
            await harness.visible(_logLines(chunk * 500, 500));
          }
          harness.resetMetrics();
        },
        run: (harness) async {
          for (var key = 0; key < 200; key++) {
            await harness.visible(String.fromCharCode(0x61 + key % 26));
          }
        },
      ),
      _Scenario(
        name: 'hidden terminal output',
        run: (harness) async {
          for (var chunk = 0; chunk < 20; chunk++) {
            await harness.hidden(_logLines(chunk * 100, 100, ansi: true));
          }
        },
      ),
      _Scenario(
        name: 'reveal after large hidden backlog',
        run: (harness) async {
          for (var chunk = 0; chunk < 16; chunk++) {
            await harness.hidden(_logLines(chunk * 500, 500, ansi: true));
          }
          await harness.reveal();
        },
      ),
      _Scenario(
        name: 'resize storm',
        setup: (harness) async {
          await harness.visible(_logLines(0, 300));
          harness.resetMetrics();
        },
        run: (harness) async {
          for (var index = 0; index < 40; index++) {
            await harness.resize(
              cols: 100 + (index % 7) * 14,
              rows: 28 + (index % 5) * 6,
            );
          }
        },
      ),
      _Scenario(
        name: 'coalesced resize storm final size',
        setup: (harness) async {
          await harness.visible(_logLines(0, 300));
          harness.resetMetrics();
        },
        run: (harness) async {
          // The production runtime now collapses the same 40-callback burst to
          // the last requested dimensions before issuing worker protocol traffic.
          await harness.resize(cols: 156, rows: 52);
        },
      ),
    ];

    // One unreported warm-up ensures isolate/JIT startup is not charged to the
    // first measured scenario while keeping every measured sample fresh-worker.
    await _measureSample(scenarios.first, warmUp: true);

    for (final scenario in scenarios) {
      final samples = <_ProfileSample>[];
      for (var run = 0; run < _measuredRuns; run++) {
        samples.add(await _measureSample(scenario));
      }
      final report = _ProfileReport(scenario.name, samples);
      // ignore: avoid_print
      print(report.format());
      expect(samples, hasLength(_measuredRuns));
      expect(samples.every((sample) => sample.workerRequests > 0), isTrue);
    }
  }, timeout: const Timeout(Duration(minutes: 3)));
}

Future<_ProfileSample> _measureSample(
  _Scenario scenario, {
  bool warmUp = false,
}) async {
  final worker = await TerminalXtermWorker.start(
    cols: _cols,
    rows: _rows,
    maxLines: _maxLines,
  );
  final replica = TerminalXtermReplicaTerminal(
    cols: _cols,
    rows: _rows,
    maxLines: _maxLines,
    notificationsEnabled: false,
  );
  final harness = _Harness(worker, replica);
  try {
    // Establish the same initial worker->replica contract as production before
    // measuring incremental traffic.
    final initial = await worker.snapshotBufferDelta();
    replica.applyBufferDelta(initial);
    harness.resetMetrics();

    if (scenario.setup case final setup?) {
      await setup(harness);
    }
    final rssBefore = ProcessInfo.currentRss;
    final wall = Stopwatch()..start();
    await scenario.run(harness);
    wall.stop();
    final rssAfter = ProcessInfo.currentRss;
    if (warmUp) {
      return harness.sample(wall.elapsed, rssAfter - rssBefore);
    }
    return harness.sample(wall.elapsed, rssAfter - rssBefore);
  } finally {
    await worker.close();
    replica.dispose();
  }
}

final class _Scenario {
  const _Scenario({required this.name, required this.run, this.setup});

  final String name;
  final Future<void> Function(_Harness harness) run;
  final Future<void> Function(_Harness harness)? setup;
}

final class _Harness {
  _Harness(this.worker, this.replica);

  final TerminalXtermWorker worker;
  final TerminalXtermReplicaTerminal replica;

  var workerMicros = 0;
  var replicaMicros = 0;
  var workerRequests = 0;
  var inputBytes = 0;
  var deltaProxyBytes = 0;
  var changedRows = 0;
  var changedCells = 0;
  var comparedRows = 0;
  var fullRepaints = 0;
  var trimRows = 0;
  var maxBufferLength = 0;
  var hiddenRequests = 0;
  var revealRequests = 0;
  var resizeRequests = 0;

  void resetMetrics() {
    workerMicros = 0;
    replicaMicros = 0;
    workerRequests = 0;
    inputBytes = 0;
    deltaProxyBytes = 0;
    changedRows = 0;
    changedCells = 0;
    comparedRows = 0;
    fullRepaints = 0;
    trimRows = 0;
    maxBufferLength = 0;
    hiddenRequests = 0;
    revealRequests = 0;
    resizeRequests = 0;
  }

  Future<void> visible(String data) async {
    final workerWatch = Stopwatch()..start();
    final delta = await worker.writeBufferDelta(data);
    workerWatch.stop();
    workerMicros += workerWatch.elapsedMicroseconds;
    workerRequests += 1;
    inputBytes += utf8.encode(data).length;
    _recordDelta(delta);

    final replicaWatch = Stopwatch()..start();
    replica.applyBufferDelta(delta);
    replicaWatch.stop();
    replicaMicros += replicaWatch.elapsedMicroseconds;
  }

  Future<void> hidden(String data) async {
    final workerWatch = Stopwatch()..start();
    final delta = await worker.parseHidden(data, focused: false);
    workerWatch.stop();
    workerMicros += workerWatch.elapsedMicroseconds;
    workerRequests += 1;
    hiddenRequests += 1;
    inputBytes += utf8.encode(data).length;

    final replicaWatch = Stopwatch()..start();
    replica.applyStateDelta(delta);
    replicaWatch.stop();
    replicaMicros += replicaWatch.elapsedMicroseconds;
  }

  Future<void> reveal() async {
    final workerWatch = Stopwatch()..start();
    final delta = await worker.snapshotBufferDelta();
    workerWatch.stop();
    workerMicros += workerWatch.elapsedMicroseconds;
    workerRequests += 1;
    revealRequests += 1;
    _recordDelta(delta);

    final replicaWatch = Stopwatch()..start();
    replica.applyBufferDelta(delta);
    replicaWatch.stop();
    replicaMicros += replicaWatch.elapsedMicroseconds;
  }

  Future<void> resize({required int cols, required int rows}) async {
    final workerWatch = Stopwatch()..start();
    final delta = await worker.resizeBufferDelta(cols: cols, rows: rows);
    workerWatch.stop();
    workerMicros += workerWatch.elapsedMicroseconds;
    workerRequests += 1;
    resizeRequests += 1;
    _recordDelta(delta);

    final replicaWatch = Stopwatch()..start();
    replica.applyBufferDelta(delta);
    replicaWatch.stop();
    replicaMicros += replicaWatch.elapsedMicroseconds;
  }

  void _recordDelta(TerminalXtermWorkerBufferDelta delta) {
    changedRows += delta.rowDeltas.length;
    changedCells += delta.rowDeltas.fold<int>(
      0,
      (sum, row) => sum + row.cells.length,
    );
    comparedRows += delta.comparedRowCount;
    if (delta.fullRepaint) fullRepaints += 1;
    trimRows += delta.trimStart;
    if (delta.bufferLength > maxBufferLength) {
      maxBufferLength = delta.bufferLength;
    }
    deltaProxyBytes += _deltaPayloadProxyBytes(delta);
  }

  _ProfileSample sample(Duration wall, int rssDeltaBytes) => _ProfileSample(
    wallMicros: wall.inMicroseconds,
    workerMicros: workerMicros,
    replicaMicros: replicaMicros,
    workerRequests: workerRequests,
    inputBytes: inputBytes,
    deltaProxyBytes: deltaProxyBytes,
    changedRows: changedRows,
    changedCells: changedCells,
    comparedRows: comparedRows,
    fullRepaints: fullRepaints,
    trimRows: trimRows,
    maxBufferLength: maxBufferLength,
    hiddenRequests: hiddenRequests,
    revealRequests: revealRequests,
    resizeRequests: resizeRequests,
    rssDeltaBytes: rssDeltaBytes,
  );
}

int _deltaPayloadProxyBytes(TerminalXtermWorkerBufferDelta delta) {
  // Stable estimate, not an isolate serialization claim: fixed scalar fields
  // are counted as eight bytes and strings as UTF-8 bytes.
  var bytes = 23 * 8;
  for (final row in delta.rowDeltas) {
    bytes += 7 * 8 + utf8.encode(row.text).length;
    for (final cell in row.cells) {
      bytes += 9 * 8;
      bytes += utf8.encode(cell.text).length;
      if (cell.combiningCharacters case final combining?) {
        bytes += utf8.encode(combining).length;
      }
    }
  }
  for (final value in delta.hyperlinkUpdates.values) {
    bytes += 8 + utf8.encode(value).length;
  }
  return bytes;
}

String _logLines(int start, int count, {bool ansi = false}) {
  final buffer = StringBuffer();
  for (var offset = 0; offset < count; offset++) {
    final index = start + offset;
    if (ansi) buffer.write(index.isEven ? '\x1b[32m' : '\x1b[33m');
    buffer.write(
      'compile[$index] package/module_${index % 73}.dart: '
      'warning: dense terminal workload ${List<String>.filled(84, 'x').join()}',
    );
    if (ansi) buffer.write('\x1b[0m');
    buffer.write('\r\n');
  }
  return buffer.toString();
}

String _agentBurst(int burst, int lines) {
  final buffer = StringBuffer();
  for (var line = 0; line < lines; line++) {
    buffer.write(
      '\x1b[36magent[$burst]\x1b[0m tool=${line % 11} '
      'status=${line.isEven ? 'running' : 'complete'} '
      '${List<String>.filled(8, 'payload-${burst}_$line ').join()}\r\n',
    );
  }
  return buffer.toString();
}

String _tuiFrame(int frame) {
  final buffer = StringBuffer('\x1b[H');
  for (var row = 0; row < _rows; row++) {
    final prefix =
        'frame=${frame.toString().padLeft(3, '0')} '
        'row=${row.toString().padLeft(2, '0')} ';
    final fill = List<String>.filled(
      _cols,
      String.fromCharCode(65 + ((frame + row) % 26)),
    ).join();
    final line = '$prefix$fill';
    buffer
      ..write('\x1b[${row + 1};1H')
      ..write(line.substring(0, _cols));
  }
  return buffer.toString();
}

final class _ProfileSample {
  const _ProfileSample({
    required this.wallMicros,
    required this.workerMicros,
    required this.replicaMicros,
    required this.workerRequests,
    required this.inputBytes,
    required this.deltaProxyBytes,
    required this.changedRows,
    required this.changedCells,
    required this.comparedRows,
    required this.fullRepaints,
    required this.trimRows,
    required this.maxBufferLength,
    required this.hiddenRequests,
    required this.revealRequests,
    required this.resizeRequests,
    required this.rssDeltaBytes,
  });

  final int wallMicros;
  final int workerMicros;
  final int replicaMicros;
  final int workerRequests;
  final int inputBytes;
  final int deltaProxyBytes;
  final int changedRows;
  final int changedCells;
  final int comparedRows;
  final int fullRepaints;
  final int trimRows;
  final int maxBufferLength;
  final int hiddenRequests;
  final int revealRequests;
  final int resizeRequests;
  final int rssDeltaBytes;
}

final class _ProfileReport {
  const _ProfileReport(this.name, this.samples);

  final String name;
  final List<_ProfileSample> samples;

  String format() {
    final workerMs = samples.map((sample) => sample.workerMicros / 1000.0);
    final replicaMs = samples.map((sample) => sample.replicaMicros / 1000.0);
    final wallMs = samples.map((sample) => sample.wallMicros / 1000.0);
    final rssMiB = samples.map(
      (sample) => sample.rssDeltaBytes / (1024.0 * 1024.0),
    );
    final requests = samples.map((sample) => sample.workerRequests).toList();
    final changedRows = samples.map((sample) => sample.changedRows).toList();
    final changedCells = samples.map((sample) => sample.changedCells).toList();
    final comparedRows = samples.map((sample) => sample.comparedRows).toList();
    final fullRepaints = samples.map((sample) => sample.fullRepaints).toList();
    final deltaProxy = samples.map((sample) => sample.deltaProxyBytes).toList();
    final input = samples.map((sample) => sample.inputBytes).toList();
    final maxBuffer = samples.map((sample) => sample.maxBufferLength).toList();

    return '\n=== T5 terminal production worker profile: $name ===\n'
        '  samples=${samples.length}; requests=${requests.join(', ')}; '
        'input bytes=${input.join(', ')}\n'
        '  wall median ${_median(wallMs).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(wallMs, 0.95).toStringAsFixed(2)} ms\n'
        '  worker roundtrip median ${_median(workerMs).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(workerMs, 0.95).toStringAsFixed(2)} ms; '
        'replica apply median ${_median(replicaMs).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(replicaMs, 0.95).toStringAsFixed(2)} ms\n'
        '  changed rows=${changedRows.join(', ')}; '
        'changed cells=${changedCells.join(', ')}; '
        'compared rows=${comparedRows.join(', ')}\n'
        '  full repaints=${fullRepaints.join(', ')}; '
        'max buffer rows=${maxBuffer.join(', ')}; '
        'delta proxy bytes=${deltaProxy.join(', ')}\n'
        '  RSS delta median ${_median(rssMiB).toStringAsFixed(2)} MiB, '
        'p95 ${_percentile(rssMiB, 0.95).toStringAsFixed(2)} MiB; '
        'heap not separately exposed by this harness\n'
        '  hidden requests=${samples.map((s) => s.hiddenRequests).join(', ')}; '
        'reveal requests=${samples.map((s) => s.revealRequests).join(', ')}; '
        'resize requests=${samples.map((s) => s.resizeRequests).join(', ')}; '
        'trim rows=${samples.map((s) => s.trimRows).join(', ')}';
  }
}

double _median(Iterable<double> values) {
  final sorted = values.toList()..sort();
  final middle = sorted.length ~/ 2;
  if (sorted.length.isOdd) return sorted[middle];
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

double _percentile(Iterable<double> values, double percentile) {
  final sorted = values.toList()..sort();
  final index = (sorted.length * percentile).ceil() - 1;
  return sorted[index.clamp(0, sorted.length - 1)];
}
