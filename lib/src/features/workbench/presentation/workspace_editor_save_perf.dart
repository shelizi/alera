import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/scheduler.dart';
import 'package:logging/logging.dart';

/// Opt-in Release Desktop Save A/B probe: absent from normal builds.
const bool kEditorSavePerfEnabled = bool.fromEnvironment(
  'ALERA_SAVE_PERF_TEST',
);

bool get editorSaveBenchmarkForceLegacy =>
    kEditorSavePerfEnabled &&
    Platform.environment['ALERA_BENCHMARK_LEGACY_SAVE'] == '1';

/// Report one Save sample after measurement has finished so file logging
/// cannot be included in the timed work.
final class EditorSavePerfProbe {
  EditorSavePerfProbe._({
    required this.native,
    required this.length,
    required this.source,
  }) : _startRss = ProcessInfo.currentRss {
    _watch.start();
    _peakRss = _startRss;
    SchedulerBinding.instance.addTimingsCallback(_onFrames);
    _rssSampler = Timer.periodic(const Duration(milliseconds: 20), (_) {
      final rss = ProcessInfo.currentRss;
      if (rss > _peakRss) _peakRss = rss;
    });
  }

  static EditorSavePerfProbe? start({
    required bool native,
    required int length,
    required String source,
  }) {
    if (!kEditorSavePerfEnabled) return null;
    return EditorSavePerfProbe._(
      native: native,
      length: length,
      source: source,
    );
  }

  final bool native;
  final int length;
  final String source;
  final int _startRss;
  late int _peakRss;
  final Stopwatch _watch = Stopwatch();
  final List<FrameTiming> _frames = <FrameTiming>[];
  Timer? _rssSampler;
  int? _prepUs;
  int? _writeUs;
  int? _writeStartUs;
  bool _finished = false;

  void markPreparationComplete() {
    _prepUs = _watch.elapsedMicroseconds;
    _writeStartUs = _prepUs;
  }

  void markWriteComplete() {
    _writeUs = _watch.elapsedMicroseconds - (_writeStartUs ?? 0);
  }

  void _onFrames(List<FrameTiming> frames) {
    _frames.addAll(frames);
  }

  void finish({required bool success}) {
    if (_finished) return;
    _finished = true;
    _watch.stop();
    final endUs = _watch.elapsedMicroseconds;
    final endRss = ProcessInfo.currentRss;
    if (endRss > _peakRss) _peakRss = endRss;

    // Wait for the engine's delayed frame reports, without lengthening endUs.
    Timer(const Duration(milliseconds: 350), () {
      _rssSampler?.cancel();
      _rssSampler = null;
      SchedulerBinding.instance.removeTimingsCallback(_onFrames);
      int? p95(Iterable<int> durations) {
        final sorted = durations.toList()..sort();
        if (sorted.isEmpty) return null;
        return sorted[((sorted.length - 1) * 0.95).round()];
      }

      final spans = _frames
          .map((frame) => frame.totalSpan.inMicroseconds)
          .toList();
      final payload = <String, Object?>{
        'mode': native ? 'native' : 'legacy',
        'source': source,
        'success': success,
        'chars': length,
        'prep_us': _prepUs,
        'write_us': _writeUs,
        'total_us': endUs,
        'rss_before': _startRss,
        'rss_after': endRss,
        'rss_peak_observed': _peakRss,
        'frames': _frames.length,
        'frame_total_p95_us': p95(spans),
        'frame_build_p95_us': p95(
          _frames.map((frame) => frame.buildDuration.inMicroseconds),
        ),
        'frame_raster_p95_us': p95(
          _frames.map((frame) => frame.rasterDuration.inMicroseconds),
        ),
        'frames_above_16ms': spans.where((us) => us > 16667).length,
      };
      Logger('alera.editor_save_benchmark')
          .info('ALERA_SAVE_BENCH ${jsonEncode(payload)}');
    });
  }
}
