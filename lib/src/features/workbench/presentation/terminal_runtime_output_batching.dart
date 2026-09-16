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

/// Floor on the gap between two flushes, so a process writing without pause
/// cannot drive the frame loop at full vsync.
///
/// Measured on Linux: a frame costs roughly the same whether it changes one
/// line or a whole screen, because the fixed per-frame cost dominates - the GTK
/// embedder reads the rendered surface back and composites it in software on
/// the platform thread (`gdk_cairo_draw_from_gl`), which no GDK setting avoids.
/// CPU therefore tracks the frame count almost linearly: 30 fps of streaming
/// output cost 48% of a core against 31% at 20 fps, with a bare frame costing
/// ~6 ms of CPU on its own. Nobody reads text scrolling at 60 Hz, so sustained
/// output uses the measured 20 fps cadence.
///
/// This is a floor on cadence, not a delay on arrival: a terminal that has been
/// quiet flushes on the very next frame, so echo latency while typing is
/// unchanged.
const Duration _terminalOutputMinFlushInterval = Duration(milliseconds: 50);
