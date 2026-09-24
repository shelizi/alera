part of 'terminal_runtime.dart';

/// Never cuts between a surrogate pair, which would corrupt the code point.
int _terminalOutputChunkCutoff(String value, int limit) {
  if (value.length <= limit) {
    return value.length;
  }
  final codeUnit = value.codeUnitAt(limit);
  if (codeUnit >= 0xDC00 && codeUnit <= 0xDFFF) {
    return limit - 1;
  }
  return limit;
}

const int _terminalOutputMaxCharsPerFrame = 64 * 1024;
const int _terminalOutputInitialAdaptiveCharsPerFrame = 16 * 1024;
const int _terminalOutputMinAdaptiveCharsPerFrame = 4 * 1024;
const int _terminalOutputAdaptiveGrowthStep = 8 * 1024;
const int _terminalOutputAdaptiveQuantum = 1024;
const Duration _terminalOutputTargetParseTime = Duration(milliseconds: 6);
const Duration _terminalOutputAdaptiveIdleResetInterval = Duration(
  milliseconds: 500,
);
const int _terminalOutputMaxPendingChars = 1024 * 1024;
const int _terminalOutputVisibleHardPendingChars = 4 * 1024 * 1024;
const int _terminalOutputHiddenCatchUpTargetChars = 256 * 1024;

/// Chooses the next saturated output-chunk budget from the measured xterm
/// parse time of the previous chunk.
///
/// Shrinking is proportional but limited to at most 2x per sample, so one
/// noisy frame cannot collapse throughput. Growth is deliberately slower: a
/// cheap chunk earns only 8 KiB at a time, which avoids oscillating between a
/// tiny ANSI-heavy budget and the 64 KiB ceiling.
int _terminalOutputNextAdaptiveChunkBudget({
  required int currentChars,
  required Duration parseTime,
}) {
  final current = currentChars
      .clamp(
        _terminalOutputMinAdaptiveCharsPerFrame,
        _terminalOutputMaxCharsPerFrame,
      )
      .toInt();
  final targetMicros = _terminalOutputTargetParseTime.inMicroseconds;
  final parseMicros = parseTime.inMicroseconds;

  if (parseMicros < targetMicros ~/ 2) {
    return (current + _terminalOutputAdaptiveGrowthStep)
        .clamp(
          _terminalOutputMinAdaptiveCharsPerFrame,
          _terminalOutputMaxCharsPerFrame,
        )
        .toInt();
  }
  if (parseMicros <= targetMicros) {
    return current;
  }

  final proportional = (current * targetMicros) ~/ parseMicros;
  final noFasterThanHalf = current ~/ 2;
  var next = proportional < noFasterThanHalf ? noFasterThanHalf : proportional;
  next = next
      .clamp(
        _terminalOutputMinAdaptiveCharsPerFrame,
        _terminalOutputMaxCharsPerFrame,
      )
      .toInt();
  next =
      (next ~/ _terminalOutputAdaptiveQuantum) * _terminalOutputAdaptiveQuantum;
  return next.clamp(_terminalOutputMinAdaptiveCharsPerFrame, current).toInt();
}

/// Selects the UI-blocking time that should drive output chunk adaptation.
///
/// Direct xterm parsing is synchronous on the UI isolate, so its wall time is
/// the blocking cost. The parser-worker backend spends most of its wall time in
/// another isolate; only rebuilding/applying the returned replica delta blocks
/// Flutter, so that narrower measurement wins when it is available.
Duration _terminalOutputAdaptiveSample({
  required Duration wallTime,
  Duration? asyncUiApplyTime,
}) {
  return asyncUiApplyTime ?? wallTime;
}

/// Converts the configured sustained-output FPS to the floor between flushes.
///
/// Measured on Linux: a frame costs roughly the same whether it changes one
/// line or a whole screen, because the fixed per-frame cost dominates - the GTK
/// embedder reads the rendered surface back and composites it in software on
/// the platform thread (`gdk_cairo_draw_from_gl`), which no GDK setting avoids.
/// CPU therefore tracks the frame count almost linearly: 30 fps of streaming
/// output cost 48% of a core against 31% at 20 fps, with a bare frame costing
/// ~6 ms of CPU on its own. The default therefore remains the measured 20 fps,
/// while power users can tune the sustained-output cadence per device.
///
/// This remains only a sustained-output floor: a quiet terminal flushes on the
/// next frame, and interactive input can explicitly promote the next live
/// output flush to the next frame.
Duration _terminalOutputFlushIntervalForFps(int fps) {
  final normalizedFps = fps.clamp(5, 120).toInt();
  return Duration(
    microseconds: (Duration.microsecondsPerSecond / normalizedFps).round(),
  );
}
