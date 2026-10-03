import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

/// How often the shared spinner advances. A 14 px arc reads as smooth at a
/// dozen steps per second, and every step costs a full-window frame: the
/// Windows Impeller backend re-rasterizes the whole window for any dirty
/// region, so a per-vsync spinner kept an otherwise idle app at ~45 frames
/// per second while an agent worked.
@visibleForTesting
const Duration agentRunSpinnerFrameInterval = Duration(milliseconds: 83);

/// Lower repaint cadence for a visible but unfocused desktop window.
/// Four steps per second keeps the liveness cue moving without restoring the
/// high full-window raster cost that the shared clock was added to avoid.
@visibleForTesting
const Duration agentRunSpinnerUnfocusedFrameInterval = Duration(
  milliseconds: 250,
);

/// One animation clock for every agent spinner under this scope.
///
/// A `CircularProgressIndicator` owns a private `AnimationController` and
/// rebuilds itself every frame, so a sidebar with twenty working agents ran
/// twenty tickers and twenty per-frame rebuilds. Sharing one clock and
/// painting it keeps the cost to one timer and N paints, with no build or
/// layout work in the list.
///
/// The clock is reference counted by the mounted spinners, so an idle sidebar
/// schedules no frames at all. It stops while hidden and drops to a low
/// repaint rate while the window is visible but unfocused.
class const AgentRunSpinnerScope({super.key, required final Widget child})
    extends StatefulWidget {
  static _AgentRunSpinnerScopeState? _maybeStateOf(BuildContext context) {
    return context
        .dependOnInheritedWidgetOfExactType<_AgentRunSpinnerAnimation>()
        ?.scope;
  }

  @override
  State<AgentRunSpinnerScope> createState() => _AgentRunSpinnerScopeState();
}

class _AgentRunSpinnerScopeState extends State<AgentRunSpinnerScope>
    with WidgetsBindingObserver {
  static const int _periodMilliseconds = 1400;

  final ValueNotifier<double> _progress = ValueNotifier<double>(0);
  final Stopwatch _clock = Stopwatch();
  Timer? _timer;
  Duration? _timerInterval;
  int _spinners = 0;
  bool _windowVisible = true;
  bool _windowFocused = true;
  bool _tickersEnabled = true;

  ValueListenable<double> get progress => _progress;

  @visibleForTesting
  bool get isRunning => _timer != null;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Same rule a ticker follows: an offstage route or a disabled subtree
    // must not keep producing frames.
    _tickersEnabled = TickerMode.valuesOf(context).enabled;
    _syncTimer();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    // On desktop, inactive commonly means visible but not focused. Keep the
    // liveness cue in that state; hidden/paused/detached still stop the clock.
    _windowVisible =
        state == AppLifecycleState.resumed ||
        state == AppLifecycleState.inactive;
    _windowFocused = state == AppLifecycleState.resumed;
    _syncTimer();
  }

  void _acquire() {
    _spinners += 1;
    _syncTimer();
  }

  void _release() {
    _spinners = math.max(0, _spinners - 1);
    _syncTimer();
  }

  void _syncTimer() {
    final interval = _spinners > 0 && _windowVisible && _tickersEnabled
        ? (_windowFocused
              ? agentRunSpinnerFrameInterval
              : agentRunSpinnerUnfocusedFrameInterval)
        : null;

    if (interval == null) {
      if (_timer == null) {
        return;
      }
      _timer!.cancel();
      _timer = null;
      _timerInterval = null;
      _clock.stop();
      return;
    }

    if (_timer != null && _timerInterval == interval) {
      return;
    }

    _timer?.cancel();
    _timerInterval = interval;
    _clock.start();
    _timer = Timer.periodic(interval, (_) {
      final phase = _clock.elapsedMilliseconds % _periodMilliseconds;
      _progress.value = phase / _periodMilliseconds;
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _timer?.cancel();
    _timer = null;
    _timerInterval = null;
    _progress.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return _AgentRunSpinnerAnimation(scope: this, child: widget.child);
  }
}

/// A plain [InheritedWidget], deliberately not an `InheritedNotifier`: the
/// latter rebuilds every dependent on each tick, which is the cost this scope
/// exists to remove. The scope identity is stable, so dependents never rebuild
/// and repaint off the shared clock instead.
class const _AgentRunSpinnerAnimation({
  required final _AgentRunSpinnerScopeState scope,
  required super.child,
}) extends InheritedWidget {
  @override
  bool updateShouldNotify(_AgentRunSpinnerAnimation oldWidget) {
    return !identical(scope, oldWidget.scope);
  }
}

/// Indeterminate spinner painted off the nearest [AgentRunSpinnerScope].
class const AgentRunSharedSpinner({
  super.key,
  required final double size,
  required final Color color,
  required final double strokeWidth,
}) extends StatefulWidget {
  /// Whether a scope is available, so callers outside the sidebar can fall
  /// back to their own indicator.
  static bool isAvailable(BuildContext context) {
    return AgentRunSpinnerScope._maybeStateOf(context) != null;
  }

  @override
  State<AgentRunSharedSpinner> createState() => _AgentRunSharedSpinnerState();
}

class _AgentRunSharedSpinnerState extends State<AgentRunSharedSpinner> {
  _AgentRunSpinnerScopeState? _scope;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final scope = AgentRunSpinnerScope._maybeStateOf(context);
    if (identical(scope, _scope)) {
      return;
    }
    _scope?._release();
    _scope = scope?.._acquire();
  }

  @override
  void dispose() {
    _scope?._release();
    _scope = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scope = _scope;
    if (scope == null) {
      return SizedBox.square(dimension: widget.size);
    }
    return RepaintBoundary(
      child: CustomPaint(
        size: .square(widget.size),
        painter: AgentRunSpinnerPainter(
          progress: scope.progress,
          color: widget.color,
          strokeWidth: widget.strokeWidth,
        ),
      ),
    );
  }
}

/// Draws the indeterminate arc for one agent run off a shared clock.
class AgentRunSpinnerPainter({
  required final ValueListenable<double> progress,
  required final Color color,
  required final double strokeWidth,
}) extends CustomPainter {
  this : super(repaint: progress);

  static const double _sweep = 4.7;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeWidth = strokeWidth
      ..color = color;
    final inset = strokeWidth / 2;
    final rect = Rect.fromLTWH(
      inset,
      inset,
      size.width - strokeWidth,
      size.height - strokeWidth,
    );
    canvas.drawArc(rect, progress.value * 2 * math.pi, _sweep, false, paint);
  }

  @override
  bool shouldRepaint(AgentRunSpinnerPainter oldDelegate) {
    return oldDelegate.color != color ||
        oldDelegate.strokeWidth != strokeWidth ||
        !identical(oldDelegate.progress, progress);
  }
}
