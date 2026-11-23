import 'dart:async';

/// Bounds Explorer's native directory and Git-status reads during a burst of
/// file-system events. A watch batch can contain many notifications from one
/// formatter, checkout or build; re-reading the same directories for every
/// notification competes with Editor and Terminal frames on the UI isolate.
class WorkspaceExplorerWatchRefreshQueue {
  WorkspaceExplorerWatchRefreshQueue({
    required this.refresh,
    this.debounce = const Duration(milliseconds: 120),
  });

  final Future<void> Function(List<String>) refresh;
  final Duration debounce;
  final Set<String> _pending = <String>{};
  Timer? _timer;
  bool _inFlight = false;
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
      _timer?.cancel();
      _timer = null;
      _drainIfReady();
      return;
    }
    if (_urgent) return;
    _timer?.cancel();
    _timer = Timer(debounce, _drainIfReady);
  }

  void _drainIfReady() {
    _timer?.cancel();
    _timer = null;
    if (_disposed || _inFlight || _pending.isEmpty) return;
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
        if (_urgent) {
          _drainIfReady();
        } else {
          _timer ??= Timer(debounce, _drainIfReady);
        }
      }
    }
  }

  /// Drop stale paths on a workspace switch while allowing an existing read
  /// to finish. No queued work will be replayed in the new workspace.
  void clear() {
    _timer?.cancel();
    _timer = null;
    _pending.clear();
    _urgent = false;
  }

  void dispose() {
    _disposed = true;
    clear();
  }
}
