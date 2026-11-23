import 'dart:async';

/// Bounds Explorer's native directory and Git-status reads during a burst of
/// file-system events. A watch batch can contain many notifications from one
/// formatter, checkout or build; re-reading the same directories for every
/// notification competes with Editor and Terminal frames on the UI isolate.
class WorkspaceExplorerWatchRefreshQueue {
  WorkspaceExplorerWatchRefreshQueue({
    required this.refresh,
    this.debounce = const Duration(milliseconds: 120),
    this.maxWait = const Duration(milliseconds: 750),
  }) : assert(maxWait > Duration.zero);

  final Future<void> Function(List<String>) refresh;
  final Duration debounce;

  /// A sustained build/formatter stream must not starve Explorer updates
  /// indefinitely by continuously resetting the short quiet-period timer.
  final Duration maxWait;
  final Set<String> _pending = <String>{};
  Timer? _timer;
  Timer? _maxWaitTimer;
  bool _inFlight = false;
  bool _overdue = false;
  bool _urgent = false;
  bool _disposed = false;

  /// Repeated paths are refreshed once. An immediate recovery refresh bypasses
  /// the debounce, including when it must wait for an in-flight read to finish.
  void enqueue(Iterable<String> directories, {bool immediate = false}) {
    if (_disposed) return;
    _pending.addAll(directories);
    if (_pending.isEmpty) return;
    if (immediate) {
      _urgent = true;
      _drainIfReady();
      return;
    }
    if (_urgent) return;
    _timer?.cancel();
    _timer = Timer(debounce, _drainIfReady);
    _maxWaitTimer ??= Timer(maxWait, _drainIfReady);
  }

  void _drainIfReady() {
    if (_disposed || _pending.isEmpty) return;
    if (_inFlight) {
      _overdue = true;
      return;
    }
    _timer?.cancel();
    _timer = null;
    _maxWaitTimer?.cancel();
    _maxWaitTimer = null;
    _overdue = false;
    _inFlight = true;
    _urgent = false;
    final batch = List<String>.unmodifiable(_pending);
    _pending.clear();
    unawaited(_run(batch));
  }

  Future<void> _run(List<String> batch) async {
    try {
      await refresh(batch);
    } catch (_) {
      // Watcher reads are best-effort; a later batch or explicit refresh can
      // recover without leaving the queue permanently stuck.
    } finally {
      _inFlight = false;
      if (!_disposed && _pending.isNotEmpty) {
        if (_urgent || _overdue) {
          _drainIfReady();
        } else {
          _timer ??= Timer(debounce, _drainIfReady);
          _maxWaitTimer ??= Timer(maxWait, _drainIfReady);
        }
      }
    }
  }

  /// Drop stale paths on a workspace switch while allowing an existing read
  /// to finish. No queued work will be replayed in the new workspace.
  void clear() {
    _timer?.cancel();
    _timer = null;
    _maxWaitTimer?.cancel();
    _maxWaitTimer = null;
    _pending.clear();
    _overdue = false;
    _urgent = false;
  }

  void dispose() {
    _disposed = true;
    clear();
  }
}
