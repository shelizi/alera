part of 'terminal_runtime.dart';

/// Callbacks the handle must supply so the accounting object can trigger
/// side-effects without holding a back-reference to the full handle.
abstract interface class _TerminalSessionVisibilityHost {
  bool get isDisposed;

  /// Called whenever `isOutputVisible` changes (lease or foreground toggle).
  ///
  /// The handle uses this to pause/resume PTY output and to schedule or cancel
  /// the pending output flush.
  void onOutputVisibilityChanged();

  /// Called when `isVisible` changes (lease acquired or released).
  ///
  /// Routes to `TerminalRuntimeSessionOwner._handleVisibilityChanged`, which
  /// in turn runs the buffer budget.
  void onVisibilityChanged();

  /// Schedules the deferred output flush for this session.
  void scheduleOutputFlush();

  /// Cancels any pending deferred flush timer, releasing the flush slot so
  /// the next output can schedule itself.
  void cancelDeferredFlush();
}

/// Per-session visibility and buffer-cost tracking.
///
/// Owns the visibility lease set, the app-foreground flag, and the
/// last-visible timestamp.  The handle delegates all lease/foreground
/// mutations here and reads `isVisible`/`isOutputVisible` back through
/// this object, so the raw fields never live on the handle directly.
class _TerminalSessionVisibilityAccounting {
  _TerminalSessionVisibilityAccounting(this._host);

  final _TerminalSessionVisibilityHost _host;

  final Set<Object> leases = <Object>{};
  bool _visible = false;
  bool _appForeground = true;
  DateTime? _lastVisibleAt;

  bool get isVisible => _visible;
  bool get isAppForeground => _appForeground;
  bool get isOutputVisible => _visible && _appForeground;

  // Setters used by dispose() in the search mixin, which clears these fields
  // directly via the mixin's abstract surface.
  set visible(bool value) => _visible = value;
  set appForeground(bool value) => _appForeground = value;

  TerminalVisibilityLease acquireLease() {
    if (_host.isDisposed) {
      return const NoopTerminalVisibilityLease();
    }
    final token = Object();
    leases.add(token);
    _syncFromLeases();
    return _TerminalVisibilityLease(() {
      if (_host.isDisposed || !leases.remove(token)) {
        return;
      }
      _syncFromLeases();
    });
  }

  void setAppForeground(bool foreground) {
    if (_host.isDisposed || _appForeground == foreground) {
      return;
    }
    _appForeground = foreground;
    _host.onOutputVisibilityChanged();
    if (isOutputVisible) {
      _host.scheduleOutputFlush();
    } else {
      _host.cancelDeferredFlush();
    }
  }

  TerminalBufferUsage estimateUsage(String tabId, xterm.Terminal terminal) {
    return TerminalBufferUsage(
      tabId: tabId,
      bytes: measureTerminalCellBufferBytes(terminal),
      lastVisibleAt: _lastVisibleAt,
    );
  }

  void _syncFromLeases() {
    final visible = leases.isNotEmpty;
    if (_visible == visible) {
      return;
    }
    _visible = visible;
    if (visible) {
      _lastVisibleAt = DateTime.now();
    }
    _host.onOutputVisibilityChanged();
    if (visible) {
      // Whatever arrived while hidden was queued without a frame callback.
      _host.scheduleOutputFlush();
    }
    _host.onVisibilityChanged();
  }
}
