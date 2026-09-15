/// Parser/model-only benchmark for xterm2 terminal state updates.
///
/// This deliberately does not mount a `TerminalView`, so the measurements isolate
/// synchronous VT parsing and cell-buffer mutation from Flutter build/raster,
/// platform embedder, and GPU work.
///
/// Deliberately not named `*_test.dart`: this is a hardware-dependent comparison
/// benchmark, not a CI gate.
///
///     flutter test integration_test/terminal_parser_benchmark.dart -d windows
///     flutter test integration_test/terminal_parser_benchmark.dart -d linux
library;

import 'dart:convert';
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:xterm2/xterm.dart' as xterm;

const _measuredRuns = 5;
const _payloadBytes = 2 * 1024 * 1024;
const _terminalColumns = 160;
const _terminalRows = 44;
const _maxLines = 10000;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('xterm parser and cell-buffer mutation cost', (tester) async {
    final scenarios = <_Scenario>[
      _Scenario('plain dense', _buildPlainPayload()),
      _Scenario('ansi heavy', _buildAnsiPayload()),
      _Scenario('tui repaint + cjk', _buildTuiPayload()),
    ];

    final reports = <_ScenarioReport>[];
    for (final scenario in scenarios) {
      // One untimed warm-up reduces one-off JIT/allocation effects in debug/profile
      // runs while leaving each measured sample with a fresh terminal buffer.
      _runSample(scenario.payload);
      final samples = <_Sample>[];
      for (var run = 0; run < _measuredRuns; run++) {
        samples.add(_runSample(scenario.payload));
        await tester.pump();
      }
      reports.add(_ScenarioReport(scenario: scenario, samples: samples));
    }

    // ignore: avoid_print
    print('\n=== terminal parser/model benchmark ===');
    for (final report in reports) {
      // ignore: avoid_print
      print(report);
      expect(report.samples, hasLength(_measuredRuns));
      expect(
        report.samples.every((sample) => sample.elapsed.inMicroseconds > 0),
        isTrue,
      );
    }
  });
}

_Sample _runSample(String payload) {
  final terminal = xterm.Terminal(maxLines: _maxLines);
  terminal.resize(_terminalColumns, _terminalRows);
  final stopwatch = Stopwatch()..start();
  terminal.write(payload);
  stopwatch.stop();
  final sample = _Sample(
    elapsed: stopwatch.elapsed,
    retainedLines: terminal.buffer.lines.length,
  );
  terminal.dispose();
  return sample;
}

String _buildPlainPayload() {
  const line =
      'build worker=07 task=compile package=workspace status=running '
      'abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ\r\n';
  return _repeatToUtf8Bytes(line, _payloadBytes);
}

String _buildAnsiPayload() {
  const line =
      '\x1b[1;34mINFO\x1b[0m compile '
      '\x1b[32mOK\x1b[0m duration=12.3ms '
      '\x1b[33mwarning\x1b[0m path=lib/src/workbench/file.dart '
      '\x1b[2K\rprogress=87%\x1b[0m\r\n';
  return _repeatToUtf8Bytes(line, _payloadBytes);
}

String _buildTuiPayload() {
  final frame = StringBuffer();
  frame.write('\x1b[?25l\x1b[H');
  for (var row = 0; row < _terminalRows; row++) {
    final color = 31 + (row % 7);
    frame
      ..write('\x1b[')
      ..write(row + 1)
      ..write(';1H\x1b[')
      ..write(color)
      ..write('m')
      ..write('狀態 ${row.toString().padLeft(2, '0')} │ 編譯中 日本語 한글 λ ✓ ')
      ..write('████████████████████████')
      ..write('\x1b[0m\x1b[K');
  }
  frame.write('\x1b[?25h');
  return _repeatToUtf8Bytes(frame.toString(), _payloadBytes);
}

String _repeatToUtf8Bytes(String unit, int targetBytes) {
  final unitBytes = utf8.encode(unit).length;
  final repeatCount = max(1, targetBytes ~/ unitBytes);
  final buffer = StringBuffer();
  for (var index = 0; index < repeatCount; index++) {
    buffer.write(unit);
  }
  final output = buffer.toString();
  final actualBytes = utf8.encode(output).length;
  // Keep scenarios within one unit of the requested size; exact byte equality is
  // less useful than avoiding a partial escape sequence at the end.
  expect((targetBytes - actualBytes).abs(), lessThan(unitBytes + 1));
  return output;
}

final class _Scenario {
  const _Scenario(this.name, this.payload);

  final String name;
  final String payload;
}

final class _Sample {
  const _Sample({required this.elapsed, required this.retainedLines});

  final Duration elapsed;
  final int retainedLines;
}

final class _ScenarioReport {
  const _ScenarioReport({required this.scenario, required this.samples});

  final _Scenario scenario;
  final List<_Sample> samples;

  double _percentileMicros(double fraction) {
    final values =
        samples
            .map((sample) => sample.elapsed.inMicroseconds.toDouble())
            .toList()
          ..sort();
    final index = ((values.length - 1) * fraction).round();
    return values[index];
  }

  @override
  String toString() {
    final bytes = utf8.encode(scenario.payload).length;
    final medianMicros = _percentileMicros(0.50);
    final p95Micros = _percentileMicros(0.95);
    final mib = bytes / (1024 * 1024);
    final medianSeconds = medianMicros / 1000000;
    final throughput = medianSeconds == 0 ? 0 : mib / medianSeconds;
    final microsPerKib = medianMicros / (bytes / 1024);
    final retained = samples.map((sample) => sample.retainedLines).join(',');
    return '${scenario.name}: bytes=$bytes runs=${samples.length} '
        'median=${(medianMicros / 1000).toStringAsFixed(2)} ms '
        'p95=${(p95Micros / 1000).toStringAsFixed(2)} ms '
        'throughput=${throughput.toStringAsFixed(2)} MiB/s '
        'parser_model=${microsPerKib.toStringAsFixed(2)} us/KiB '
        'retained_lines=[$retained]';
  }
}
