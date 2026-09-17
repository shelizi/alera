import 'dart:convert';

import 'package:alera/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

const _cols = 160;
const _rows = 44;
const _maxLines = 12_000;
const _measuredRuns = 5;

void main() {
  test('hidden backlog reveal stage profile', () async {
    await _measure(warmUp: true);
    await _measure(warmUp: true, packed: true);
    final baseline = <_RevealSample>[];
    final packed = <_RevealSample>[];
    for (var run = 0; run < _measuredRuns; run++) {
      baseline.add(await _measure());
      packed.add(await _measure(packed: true));
    }

    _printReport('nested object rows', baseline);
    _printReport('packed transferable rows', packed);

    expect(baseline, hasLength(_measuredRuns));
    expect(packed, hasLength(_measuredRuns));
    expect(
      <_RevealSample>[
        ...baseline,
        ...packed,
      ].every((sample) => sample.changedRows == 8001),
      isTrue,
    );
    expect(
      <_RevealSample>[
        ...baseline,
        ...packed,
      ].every((sample) => sample.changedCells > 1_000_000),
      isTrue,
    );
  }, timeout: const Timeout(Duration(minutes: 3)));
}

void _printReport(String name, List<_RevealSample> samples) {
  int median(int Function(_RevealSample sample) select) {
    final values = samples.map(select).toList()..sort();
    return values[values.length ~/ 2];
  }

  // ignore: avoid_print
  print('''
=== T6 hidden backlog reveal pipeline: $name ===
  samples=${samples.length}
  hidden input bytes=${samples.first.hiddenInputBytes}
  rows=${samples.first.changedRows}; cells=${samples.first.changedCells}
  raw roundtrip median ${_ms(median((s) => s.rawRoundtripMicros))} ms
  worker materialize median ${_ms(median((s) => s.materializeMicros))} ms
  isolate transfer/scheduling median ${_ms(median((s) => s.transferAndSchedulingMicros))} ms
  UI decode median ${_ms(median((s) => s.decodeMicros))} ms
  replica apply median ${_ms(median((s) => s.replicaApplyMicros))} ms
  reveal end-to-end median ${_ms(median((s) => s.endToEndMicros))} ms
''');
}

Future<_RevealSample> _measure({
  bool warmUp = false,
  bool packed = false,
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
  try {
    replica.applyBufferDelta(await worker.snapshotBufferDelta());
    var hiddenInputBytes = 0;
    for (var chunk = 0; chunk < 16; chunk++) {
      final data = _logLines(chunk * 500, 500, ansi: true);
      hiddenInputBytes += utf8.encode(data).length;
      replica.applyStateDelta(await worker.parseHidden(data, focused: false));
    }

    final revealWatch = Stopwatch()..start();
    final profile = packed
        ? await worker.profilePackedSnapshotBufferDelta()
        : await worker.profileSnapshotBufferDelta();
    final applyWatch = Stopwatch()..start();
    replica.applyBufferDelta(profile.delta);
    applyWatch.stop();
    revealWatch.stop();

    final changedCells = profile.delta.rowDeltas.fold<int>(
      0,
      (sum, row) => sum + row.cells.length,
    );
    final sample = _RevealSample(
      hiddenInputBytes: hiddenInputBytes,
      changedRows: profile.delta.rowDeltas.length,
      changedCells: changedCells,
      rawRoundtripMicros: profile.rawRoundtripMicros,
      materializeMicros: profile.materializeMicros,
      transferAndSchedulingMicros: profile.transferAndSchedulingMicros,
      decodeMicros: profile.decodeMicros,
      replicaApplyMicros: applyWatch.elapsedMicroseconds,
      endToEndMicros: revealWatch.elapsedMicroseconds,
    );
    if (!warmUp) return sample;
    return sample;
  } finally {
    await worker.close();
    replica.dispose();
  }
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

String _ms(int micros) => (micros / 1000).toStringAsFixed(2);

final class _RevealSample {
  const _RevealSample({
    required this.hiddenInputBytes,
    required this.changedRows,
    required this.changedCells,
    required this.rawRoundtripMicros,
    required this.materializeMicros,
    required this.transferAndSchedulingMicros,
    required this.decodeMicros,
    required this.replicaApplyMicros,
    required this.endToEndMicros,
  });

  final int hiddenInputBytes;
  final int changedRows;
  final int changedCells;
  final int rawRoundtripMicros;
  final int materializeMicros;
  final int transferAndSchedulingMicros;
  final int decodeMicros;
  final int replicaApplyMicros;
  final int endToEndMicros;
}
