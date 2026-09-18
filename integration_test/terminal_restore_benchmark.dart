/// Snapshot-replay benchmark for the production parser-worker terminal surface.
///
/// This emits live output immediately behind a default-size snapshot, which is
/// the production ordering that used to evict the snapshot and leave the
/// "Restoring Terminal" overlay waiting for minutes.
///
///     flutter test integration_test/terminal_restore_benchmark.dart -d windows
///
/// Deliberately not named `*_test.dart`: this is a hardware-dependent
/// measurement tool, not a CI check. The three-second target applies to the
/// default 2.56 MB replay after its event reaches Flutter, not to host attach.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/terminal_surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ghostty_vte_flutter/ghostty_vte_flutter.dart';
import 'package:integration_test/integration_test.dart';

const _snapshotBytes = 2_560_000;
const _liveOutputBytes = 1024 * 1024;
const _measuredRuns = 5;
const _restoreTarget = Duration(seconds: 3);
const _watchdog = Duration(seconds: 30);
const _restoreMarker = 'RESTORE-END';
const _liveMarkerPrefix = 'LIVE-AFTER-SNAP-';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('default terminal snapshot replay latency', (tester) async {
    binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
    final fakeSession = _BenchmarkPtySession();
    final runtime = XtermTerminalRuntime(
      parserWorkerEnabled: true,
      snapshotHydrationProfilingEnabled: true,
      ptySessionFactory: _BenchmarkPtySessionFactory(fakeSession),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        const GhosttyTerminalShellLaunch(
          label: 'Benchmark',
          shell: '/bin/sh',
          environment: <String, String>{'TERM': 'xterm-256color'},
        ),
      ],
    );
    var runtimeDisposed = false;
    addTearDown(() {
      if (!runtimeDisposed) runtime.dispose();
    });
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    expect(terminalParserWorkerEnabledForTesting(session), isTrue);

    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          home: Scaffold(body: TerminalSurface(session: session)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(fakeSession.started, isTrue);

    final snapshot = _buildSnapshot();
    expect(snapshot.length, _snapshotBytes);
    final warmupMarker = _liveMarkerForRun(0);
    final warmupLiveOutput = _buildLiveOutput(warmupMarker);
    expect(utf8.encode(warmupLiveOutput), hasLength(_liveOutputBytes));

    await _measureRestore(
      tester: tester,
      binding: binding,
      fakeSession: fakeSession,
      session: session,
      snapshot: snapshot,
      liveOutput: warmupLiveOutput,
      liveMarker: warmupMarker,
    );
    final samples = <_RestoreSample>[];
    for (var run = 0; run < _measuredRuns; run++) {
      final liveMarker = _liveMarkerForRun(run + 1);
      final liveOutput = _buildLiveOutput(liveMarker);
      expect(utf8.encode(liveOutput), hasLength(_liveOutputBytes));
      samples.add(
        await _measureRestore(
          tester: tester,
          binding: binding,
          fakeSession: fakeSession,
          session: session,
          snapshot: snapshot,
          liveOutput: liveOutput,
          liveMarker: liveMarker,
        ),
      );
    }

    final report = _RestoreReport(samples);
    // ignore: avoid_print
    print(report.format(snapshot));
    expect(samples, hasLength(_measuredRuns));
    expect(samples.every((sample) => sample.frames.isNotEmpty), isTrue);
    // The measurement deliberately leaves live output queued. Retire its frame
    // callback before the test binding checks for outstanding animations.
    await tester.pumpWidget(const SizedBox());
    runtime.dispose();
    runtimeDisposed = true;
    await tester.pump();
  });

  testWidgets('T8e soft eviction structured reveal latency', (tester) async {
    binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
    final fakeSession = _BenchmarkPtySession();
    final runtime = XtermTerminalRuntime(
      parserWorkerEnabled: true,
      ptySessionFactory: _BenchmarkPtySessionFactory(fakeSession),
      shellLaunchesBuilder: () => <GhosttyTerminalShellLaunch>[
        const GhosttyTerminalShellLaunch(
          label: 'Benchmark',
          shell: '/bin/sh',
          environment: <String, String>{'TERM': 'xterm-256color'},
        ),
      ],
    );
    addTearDown(runtime.dispose);
    final session = runtime.sessionFor(workspace: _workspace(), tab: _tab());
    expect(terminalParserWorkerEnabledForTesting(session), isTrue);

    final snapshot = _buildSnapshot();
    final snapshotText = const Utf8Decoder(allowMalformed: true)
        .convert(snapshot);
    rebuildTerminalFromSnapshotTextForTesting(session, snapshotText);
    await waitForTerminalParserApplyForTesting(session);
    expect(terminalBufferTextForTesting(session), contains(_restoreMarker));

    Future<double> measureReveal() async {
      evictTerminalSessionForTesting(runtime, session.tabId);
      expect(terminalUiBufferEvictedForTesting(session), isTrue);
      expect(session.bufferUsage.bytes, 0);

      final watch = Stopwatch()..start();
      final visibility = acquireTerminalVisibilityForTesting(session);
      await waitForTerminalParserApplyForTesting(session);
      await tester.pump();
      watch.stop();

      expect(terminalUiBufferEvictedForTesting(session), isFalse);
      expect(terminalBufferTextForTesting(session), contains(_restoreMarker));
      visibility.dispose();
      return watch.elapsedMicroseconds / 1000;
    }

    await measureReveal();
    final samples = <double>[
      for (var run = 0; run < _measuredRuns; run++) await measureReveal(),
    ];
    final sorted = List<double>.from(samples)..sort();
    final median = _median(samples);
    final p95 = sorted[(sorted.length * 0.95).ceil() - 1];
    final withinTarget = samples.where((value) => value <= 3000).length;
    // ignore: avoid_print
    print(
      '=== T8e soft eviction structured reveal ===\n'
      '  samples ${samples.map((value) => value.toStringAsFixed(2)).join(', ')} ms\n'
      '  median ${median.toStringAsFixed(2)} ms; '
      'p95 ${p95.toStringAsFixed(2)} ms; '
      'target max 3000 ms: $withinTarget/${samples.length}',
    );
    expect(samples, hasLength(_measuredRuns));
    expect(withinTarget, _measuredRuns);
  });
}

Future<_RestoreSample> _measureRestore({
  required WidgetTester tester,
  required IntegrationTestWidgetsFlutterBinding binding,
  required _BenchmarkPtySession fakeSession,
  required TerminalSessionHandle session,
  required Uint8List snapshot,
  required String liveOutput,
  required String liveMarker,
}) async {
  final watch = Stopwatch();
  Duration? accepted;
  Duration? firstChunk;
  final frameworkReady = Completer<void>();
  var sawProgress = false;

  void observeProgress() {
    final progress = session.restoreProgress.value;
    if (progress != null) {
      sawProgress = true;
      accepted ??= watch.elapsed;
      if (progress.writtenChars > 0) {
        firstChunk ??= watch.elapsed;
      }
      return;
    }
    if (!sawProgress || frameworkReady.isCompleted) {
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!frameworkReady.isCompleted) {
        frameworkReady.complete();
      }
    });
  }

  final frames = <FrameTiming>[];
  void collectFrames(List<FrameTiming> timings) => frames.addAll(timings);

  session.restoreProgress.addListener(observeProgress);
  binding.addTimingsCallback(collectFrames);
  final flushesBefore = terminalOutputFlushCountForTesting(session);
  final cpuBefore = _processCpuSeconds();
  try {
    watch.start();
    fakeSession.emit(TerminalPtySnapshotEvent(snapshot));
    fakeSession.emit(TerminalPtyOutputTextEvent(liveOutput));

    await tester.runAsync(() => frameworkReady.future.timeout(_watchdog));
    watch.stop();
    final elapsed = watch.elapsed;
    final cpuAfter = _processCpuSeconds();
    final flushes = terminalOutputFlushCountForTesting(session) - flushesBefore;
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 50)),
    );

    var text = terminalBufferTextForTesting(session);
    final restoreOffset = text.indexOf(_restoreMarker);
    final pendingLiveAtRestoreReady = pendingLiveTerminalOutputCharsForTesting(
      session,
    );
    final pendingTotalAtRestoreReady = pendingTerminalOutputCharsForTesting(
      session,
    );
    final revisionAtRestoreReady = terminalParserWorkerRevisionForTesting(
      session,
    );
    final writeInFlightAtRestoreReady = terminalOutputWriteInFlightForTesting(
      session,
    );
    final flushesAtRestoreReady = terminalOutputFlushCountForTesting(session);
    expect(sawProgress, isTrue);
    expect(accepted, isNotNull);
    expect(firstChunk, isNotNull);
    expect(session.restoreProgress.value, isNull);
    expect(terminalPointerInputSuspendedForTesting(session), isFalse);
    expect(find.text('Restoring Terminal'), findsNothing);
    expect(restoreOffset, greaterThanOrEqualTo(0));
    expect(pendingLiveAtRestoreReady, greaterThan(0));
    final hydrationProfile = terminalSnapshotHydrationProfileForTesting(
      session,
    );
    expect(hydrationProfile, isNotNull);
    final snapshotRevision = hydrationProfile!.snapshotRevision;
    expect(revisionAtRestoreReady, greaterThanOrEqualTo(snapshotRevision));

    final liveDeadline = DateTime.now().add(const Duration(seconds: 3));
    var liveOffset = text.indexOf(liveMarker);
    while (liveOffset < 0 && DateTime.now().isBefore(liveDeadline)) {
      await tester.pump(const Duration(milliseconds: 16));
      text = terminalBufferTextForTesting(session);
      liveOffset = text.indexOf(liveMarker);
    }
    if (liveOffset < 0) {
      final pendingLiveAtDeadline = pendingLiveTerminalOutputCharsForTesting(
        session,
      );
      final pendingTotalAtDeadline = pendingTerminalOutputCharsForTesting(
        session,
      );
      final revisionAtDeadline = terminalParserWorkerRevisionForTesting(
        session,
      );
      final writeInFlightAtDeadline = terminalOutputWriteInFlightForTesting(
        session,
      );
      final flushesAtDeadline = terminalOutputFlushCountForTesting(session);
      debugPrint(
        'T8D_RESTORE_DIAG '
        'readyLive=$pendingLiveAtRestoreReady '
        'readyTotal=$pendingTotalAtRestoreReady '
        'readyRevision=$revisionAtRestoreReady '
        'readyInFlight=$writeInFlightAtRestoreReady '
        'readyFlushes=$flushesAtRestoreReady '
        'deadlineLive=$pendingLiveAtDeadline '
        'deadlineTotal=$pendingTotalAtDeadline '
        'deadlineRevision=$revisionAtDeadline '
        'deadlineInFlight=$writeInFlightAtDeadline '
        'deadlineFlushes=$flushesAtDeadline',
      );
    }
    expect(liveOffset, greaterThanOrEqualTo(0));
    final liveRevision = terminalParserWorkerRevisionForTesting(session);
    expect(liveRevision, greaterThan(snapshotRevision));

    return _RestoreSample(
      accepted: accepted!,
      firstChunk: firstChunk!,
      frameworkReady: elapsed,
      hydrationProfile: hydrationProfile,
      flushes: flushes,
      cpuSeconds: cpuBefore == null || cpuAfter == null
          ? null
          : cpuAfter - cpuBefore,
      frames: List<FrameTiming>.unmodifiable(frames),
    );
  } finally {
    if (watch.isRunning) {
      watch.stop();
    }
    binding.removeTimingsCallback(collectFrames);
    session.restoreProgress.removeListener(observeProgress);
  }
}

Uint8List _buildSnapshot() {
  const styledToken = '\x1b[32mOK\x1b[0m';
  final body = List<String>.filled(21, styledToken).join();
  final records = List<String>.generate(10_000, (index) {
    final suffix = index == 9_999
        ? _restoreMarker
        : 'ROW-${index.toString().padLeft(5, '0')}';
    return '$body${suffix.padRight(23, '.')}\r\n';
  });
  return Uint8List.fromList(utf8.encode(records.join()));
}

String _liveMarkerForRun(int run) =>
    '$_liveMarkerPrefix${run.toString().padLeft(3, '0')}';

String _buildLiveOutput(String liveMarker) {
  final prefix = '\r\n$liveMarker\r\n';
  const control = '\x1b[0m';
  final bodyBytes = _liveOutputBytes - prefix.length;
  final controls = List<String>.filled(bodyBytes ~/ control.length, control);
  final padding = List<String>.filled(bodyBytes % control.length, ' ');
  return '$prefix${controls.join()}${padding.join()}';
}

double? _processCpuSeconds() {
  if (!Platform.isLinux) {
    return null;
  }
  try {
    final stat = File('/proc/self/stat').readAsStringSync();
    final fields = stat.substring(stat.lastIndexOf(')') + 2).split(' ');
    return (int.parse(fields[11]) + int.parse(fields[12])) / 100;
  } catch (_) {
    return null;
  }
}

double _median(Iterable<double> values) {
  final sorted = values.toList()..sort();
  final middle = sorted.length ~/ 2;
  if (sorted.length.isOdd) {
    return sorted[middle];
  }
  return (sorted[middle - 1] + sorted[middle]) / 2;
}

double _percentile(Iterable<double> values, double percentile) {
  final sorted = values.toList()..sort();
  final index = (sorted.length * percentile).ceil() - 1;
  return sorted[index.clamp(0, sorted.length - 1)];
}

double _milliseconds(Duration duration) =>
    duration.inMicroseconds / Duration.microsecondsPerMillisecond;

final class _RestoreSample {
  const _RestoreSample({
    required this.accepted,
    required this.firstChunk,
    required this.frameworkReady,
    required this.hydrationProfile,
    required this.flushes,
    required this.cpuSeconds,
    required this.frames,
  });

  final Duration accepted;
  final Duration firstChunk;
  final Duration frameworkReady;
  final TerminalSnapshotHydrationProfileForTesting hydrationProfile;
  final int flushes;
  final double? cpuSeconds;
  final List<FrameTiming> frames;
}

final class _RestoreReport {
  const _RestoreReport(this.samples);

  final List<_RestoreSample> samples;

  String format(Uint8List snapshot) {
    final ready = samples
        .map((sample) => _milliseconds(sample.frameworkReady))
        .toList();
    final accepted = samples
        .map((sample) => _milliseconds(sample.accepted))
        .toList();
    final firstChunk = samples
        .map((sample) => _milliseconds(sample.firstChunk))
        .toList();
    final readyMedian = _median(ready);
    final queueAndStartup = samples
        .map((sample) => sample.hydrationProfile.queueAndStartupMicros / 1000)
        .toList();
    final workerParse = samples
        .map((sample) => sample.hydrationProfile.workerParseMicros / 1000)
        .toList();
    final workerMaterialize = samples
        .map((sample) => sample.hydrationProfile.workerMaterializeMicros / 1000)
        .toList();
    final transferAndScheduling = samples
        .map(
          (sample) =>
              sample.hydrationProfile.transferAndSchedulingMicros / 1000,
        )
        .toList();
    final decode = samples
        .map((sample) => sample.hydrationProfile.decodeMicros / 1000)
        .toList();
    final uiApply = samples
        .map((sample) => sample.hydrationProfile.uiApplyMicros / 1000)
        .toList();
    final hydrationTotal = samples
        .map((sample) => sample.hydrationProfile.totalHydrationMicros / 1000)
        .toList();
    final postHydration = <double>[
      for (var i = 0; i < samples.length; i++)
        (ready[i] - accepted[i] - hydrationTotal[i]).clamp(0, double.infinity),
    ];
    final deviations = ready
        .map((value) => (value - readyMedian).abs())
        .toList();
    final builds = <double>[
      for (final sample in samples)
        for (final frame in sample.frames)
          frame.buildDuration.inMicroseconds / 1000,
    ];
    final rasters = <double>[
      for (final sample in samples)
        for (final frame in sample.frames)
          frame.rasterDuration.inMicroseconds / 1000,
    ];
    final cpu = <double>[
      for (final sample in samples)
        if (sample.cpuSeconds case final seconds?)
          seconds * 100 / (sample.frameworkReady.inMicroseconds / 1000000),
    ];
    final withinTarget = samples
        .where((sample) => sample.frameworkReady <= _restoreTarget)
        .length;
    final escapeCount = snapshot.where((byte) => byte == 0x1B).length;
    final throughput = (_snapshotBytes / (1024 * 1024)) / (readyMedian / 1000);
    final slowFrames = <FrameTiming>[
      for (final sample in samples)
        ...sample.frames.where(
          (frame) => frame.totalSpan > const Duration(microseconds: 16_700),
        ),
    ];

    return '\n=== terminal snapshot replay ===\n'
        '  snapshot ${snapshot.length} bytes, $escapeCount ESC characters; '
        'live backlog $_liveOutputBytes bytes\n'
        '  accepted median ${_median(accepted).toStringAsFixed(2)} ms; '
        'first chunk median ${_median(firstChunk).toStringAsFixed(2)} ms\n'
        '  framework post-frame median ${readyMedian.toStringAsFixed(2)} ms, '
        'p95 ${_percentile(ready, 0.95).toStringAsFixed(2)} ms, '
        'max ${ready.reduce((a, b) => a > b ? a : b).toStringAsFixed(2)} ms, '
        'MAD ${_median(deviations).toStringAsFixed(2)} ms\n'
        '  target max 3000 ms: $withinTarget/${samples.length}; '
        'throughput ${throughput.toStringAsFixed(2)} MiB/s\n'
        '  hydrate medians: queue/startup '
        '${_median(queueAndStartup).toStringAsFixed(2)} ms; worker parse '
        '${_median(workerParse).toStringAsFixed(2)} ms; packed materialize '
        '${_median(workerMaterialize).toStringAsFixed(2)} ms; '
        'transfer/scheduling '
        '${_median(transferAndScheduling).toStringAsFixed(2)} ms; decode '
        '${_median(decode).toStringAsFixed(2)} ms; UI apply '
        '${_median(uiApply).toStringAsFixed(2)} ms; total '
        '${_median(hydrationTotal).toStringAsFixed(2)} ms; post-hydration '
        '${_median(postHydration).toStringAsFixed(2)} ms\n'
        '  flushes ${samples.map((sample) => sample.flushes).join(', ')}; '
        'frames ${samples.fold<int>(0, (sum, sample) => sum + sample.frames.length)}, '
        'slow frames ${slowFrames.length}\n'
        '  cpu ${cpu.isEmpty ? '?' : '${_median(cpu).toStringAsFixed(1)}%'} '
        'of a core; build median '
        '${builds.isEmpty ? '?' : _median(builds).toStringAsFixed(2)} ms, '
        'p95 ${builds.isEmpty ? '?' : _percentile(builds, 0.95).toStringAsFixed(2)} ms; '
        'raster median '
        '${rasters.isEmpty ? '?' : _median(rasters).toStringAsFixed(2)} ms, '
        'p95 ${rasters.isEmpty ? '?' : _percentile(rasters, 0.95).toStringAsFixed(2)} ms';
  }
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 7, 27);
  return Workspace(
    id: 'workspace-restore-bench',
    projectId: 'project-restore-bench',
    name: 'Restore Bench',
    branch: 'main',
    path: '/repo/restore-bench',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab() {
  final now = DateTime.utc(2026, 7, 27);
  return WorkspaceTabRecord(
    id: 'tab-restore-bench',
    workspaceId: 'workspace-restore-bench',
    title: 'Restore Bench',
    createdAt: now,
    updatedAt: now,
    payload: const <String, Object?>{},
  );
}

final class _BenchmarkPtySessionFactory implements TerminalPtySessionFactory {
  const _BenchmarkPtySessionFactory(this.session);

  final _BenchmarkPtySession session;

  @override
  TerminalPtySession create({
    required String sessionId,
    required String workspaceId,
    required String tabId,
  }) => session;
}

final class _BenchmarkPtySession implements TerminalPtySession {
  final StreamController<TerminalPtySessionEvent> _events =
      StreamController<TerminalPtySessionEvent>.broadcast(sync: true);

  bool started = false;

  @override
  Stream<TerminalPtySessionEvent> get events => _events.stream;

  @override
  bool get startedNewProcess => false;

  @override
  Future<void> start({
    required GhosttyTerminalShellLaunch launch,
    required String workingDirectory,
    required int cols,
    required int rows,
    Future<void> Function()? onProcessCreated,
  }) async {
    started = true;
  }

  void emit(TerminalPtySessionEvent event) => _events.add(event);

  @override
  bool writeBytes(List<int> bytes) => bytes.isNotEmpty;

  @override
  Future<bool> writeBytesAndWait(List<int> bytes) async => writeBytes(bytes);

  @override
  void resize(int cols, int rows, int cellWidthPx, int cellHeightPx) {}

  @override
  Future<void> refreshViewport(
    int cols,
    int rows,
    int cellWidthPx,
    int cellHeightPx,
  ) async {}

  @override
  Future<void> setOutputPaused(bool paused) async {}

  @override
  void dispose() {
    if (!_events.isClosed) {
      unawaited(_events.close());
    }
  }

  @override
  void terminate() => dispose();
}
