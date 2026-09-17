/// Production parser-worker streaming/render benchmark.
///
/// Output is fed through [XtermTerminalRuntime] while the production parser
/// worker is explicitly enabled. Five comparable samples report scheduling,
/// worker catch-up, build/raster timing, frame jank, and RSS movement.
///
///     flutter test test/benchmarks/terminal_streaming_render_profile_benchmark.dart
///
/// Deliberately not named `*_test.dart`: this is an opt-in flutter_tester
/// profile gate, not a CI performance assertion or Windows desktop result.
library;

import 'dart:io';
import 'dart:math';

import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_test/flutter_test.dart';

const _sampleDuration = Duration(seconds: 3);
const _writeInterval = Duration(milliseconds: 8);
const _lineWidth = 200;
const _measuredRuns = 5;
const _jankThreshold = Duration(microseconds: 16_700);

double? _processCpuSeconds() {
  if (!Platform.isLinux) return null;
  try {
    final stat = File('/proc/self/stat').readAsStringSync();
    final fields = stat.substring(stat.lastIndexOf(')') + 2).split(' ');
    return (int.parse(fields[11]) + int.parse(fields[12])) / 100;
  } catch (_) {
    return null;
  }
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 9, 17);
  return Workspace(
    id: 'workspace-production-terminal-bench',
    projectId: 'project-production-terminal-bench',
    name: 'Production Terminal Bench',
    branch: 'main',
    path: '/repo/production-terminal-bench',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab() {
  final now = DateTime.utc(2026, 9, 17);
  return WorkspaceTabRecord(
    id: 'tab-production-terminal-bench',
    workspaceId: 'workspace-production-terminal-bench',
    title: 'Production Terminal Bench',
    createdAt: now,
    updatedAt: now,
    payload: const <String, Object?>{},
  );
}

void main() {
  final binding = LiveTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('production terminal streaming/render profile gate', (
    tester,
  ) async {
    binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

    final runtime = XtermTerminalRuntime(parserWorkerEnabled: true);
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    expect(terminalParserWorkerEnabledForTesting(session), isTrue);

    await tester.pumpWidget(
      MaterialApp(
        debugShowCheckedModeBanner: false,
        home: Scaffold(
          backgroundColor: const Color(0xFF0B0D10),
          body: session.buildView(),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final visibility = acquireTerminalVisibilityForTesting(session);
    addTearDown(visibility.dispose);

    final random = Random(7);
    final line = List<String>.generate(
      _lineWidth,
      (_) => String.fromCharCode(33 + random.nextInt(90)),
    ).join();

    // Warm parser/model, paragraph cache, and renderer before collecting the
    // five comparable samples.
    await tester.runAsync(() async {
      for (var i = 0; i < 60; i++) {
        queueTerminalOutputForTesting(session, '$i $line\r\n');
        await Future<void>.delayed(_writeInterval);
      }
      await waitForTerminalParserApplyForTesting(session);
    });

    final samples = <_StreamingSample>[];
    for (var run = 0; run < _measuredRuns; run++) {
      samples.add(
        await _measureStreamingSample(
          tester: tester,
          binding: binding,
          session: session,
          line: line,
          run: run,
        ),
      );
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
    }

    final report = _StreamingReport(samples);
    // ignore: avoid_print
    print(report.format());
    expect(samples, hasLength(_measuredRuns));
    expect(samples.every((sample) => sample.frames.isNotEmpty), isTrue);
  });
}

Future<_StreamingSample> _measureStreamingSample({
  required WidgetTester tester,
  required LiveTestWidgetsFlutterBinding binding,
  required TerminalSessionHandle session,
  required String line,
  required int run,
}) async {
  final timings = <FrameTiming>[];
  void collect(List<FrameTiming> frames) => timings.addAll(frames);

  final flushesBefore = terminalOutputFlushCountForTesting(session);
  final rssBefore = ProcessInfo.currentRss;
  final cpuBefore = _processCpuSeconds();
  binding.addTimingsCallback(collect);
  final watch = Stopwatch()..start();
  var writes = 0;
  try {
    await tester.runAsync(() async {
      while (watch.elapsed < _sampleDuration) {
        queueTerminalOutputForTesting(
          session,
          'run=$run write=${writes++} $line\r\n',
        );
        await Future<void>.delayed(_writeInterval);
      }
      await waitForTerminalParserApplyForTesting(session);
    });
    watch.stop();
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 50)),
    );
  } finally {
    if (watch.isRunning) watch.stop();
    binding.removeTimingsCallback(collect);
  }

  final cpuAfter = _processCpuSeconds();
  return _StreamingSample(
    elapsed: watch.elapsed,
    writes: writes,
    flushes: terminalOutputFlushCountForTesting(session) - flushesBefore,
    frames: List<FrameTiming>.unmodifiable(timings),
    rssDeltaBytes: ProcessInfo.currentRss - rssBefore,
    cpuSeconds: cpuBefore == null || cpuAfter == null
        ? null
        : cpuAfter - cpuBefore,
  );
}

final class _StreamingSample {
  const _StreamingSample({
    required this.elapsed,
    required this.writes,
    required this.flushes,
    required this.frames,
    required this.rssDeltaBytes,
    required this.cpuSeconds,
  });

  final Duration elapsed;
  final int writes;
  final int flushes;
  final List<FrameTiming> frames;
  final int rssDeltaBytes;
  final double? cpuSeconds;
}

final class _StreamingReport {
  const _StreamingReport(this.samples);

  final List<_StreamingSample> samples;

  String format() {
    final writesPerSecond = samples.map(
      (sample) => sample.writes / _seconds(sample.elapsed),
    );
    final flushesPerSecond = samples.map(
      (sample) => sample.flushes / _seconds(sample.elapsed),
    );
    final framesPerSecond = samples.map(
      (sample) => sample.frames.length / _seconds(sample.elapsed),
    );
    final build = <double>[
      for (final sample in samples)
        for (final frame in sample.frames)
          frame.buildDuration.inMicroseconds / 1000.0,
    ];
    final raster = <double>[
      for (final sample in samples)
        for (final frame in sample.frames)
          frame.rasterDuration.inMicroseconds / 1000.0,
    ];
    final total = <double>[
      for (final sample in samples)
        for (final frame in sample.frames)
          frame.totalSpan.inMicroseconds / 1000.0,
    ];
    final jankCounts = samples
        .map(
          (sample) => sample.frames
              .where((frame) => frame.totalSpan > _jankThreshold)
              .length,
        )
        .toList();
    final rssMiB = samples.map(
      (sample) => sample.rssDeltaBytes / (1024.0 * 1024.0),
    );
    final cpuPercent = <double>[
      for (final sample in samples)
        if (sample.cpuSeconds case final cpu?)
          cpu * 100 / _seconds(sample.elapsed),
    ];

    return '\n=== T5 production terminal streaming/render profile ===\n'
        '  samples=${samples.length}; writes/s median '
        '${_median(writesPerSecond).toStringAsFixed(1)}, p95 '
        '${_percentile(writesPerSecond, 0.95).toStringAsFixed(1)}\n'
        '  flushes/s median ${_median(flushesPerSecond).toStringAsFixed(1)}, '
        'p95 ${_percentile(flushesPerSecond, 0.95).toStringAsFixed(1)}; '
        'frames/s median ${_median(framesPerSecond).toStringAsFixed(1)}\n'
        '  build median ${_median(build).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(build, 0.95).toStringAsFixed(2)} ms; '
        'raster median ${_median(raster).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(raster, 0.95).toStringAsFixed(2)} ms\n'
        '  total frame median ${_median(total).toStringAsFixed(2)} ms, '
        'p95 ${_percentile(total, 0.95).toStringAsFixed(2)} ms; '
        'jank frames/sample=${jankCounts.join(', ')}\n'
        '  RSS delta median ${_median(rssMiB).toStringAsFixed(2)} MiB, '
        'p95 ${_percentile(rssMiB, 0.95).toStringAsFixed(2)} MiB; '
        'cpu ${cpuPercent.isEmpty ? '?' : '${_median(cpuPercent).toStringAsFixed(1)}% of a core'}';
  }
}

double _seconds(Duration duration) =>
    duration.inMicroseconds / Duration.microsecondsPerSecond;

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
