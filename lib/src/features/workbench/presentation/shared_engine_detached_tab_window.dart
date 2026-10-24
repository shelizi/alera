import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/feedback/alera_toast_host.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/text_actions/presentation/text_actions_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:multiview_desktop/multiview_desktop.dart';
import 'package:xterm2/ui.dart' show EarlyInputAttachScope;

part 'shared_engine_detached_tab_window_view.dart';

/// Windows-only backend. Registered only if the application's root is
/// runMultiApp and all views share the same root ProviderScope.
///
/// Editor always uses this backend on Windows. Terminal does too when
/// [experimentalTerminalTabs] is true. Production passes the compile-time
/// `experimentalSharedTerminalTabs` constant, whose default is true. The
/// constructor default stays false so a compile-time override and tests can
/// keep Terminal on the child process. Git Diff, PDF and other types retain
/// the process-backed path.
final class SharedEngineDetachedTabWindows extends ChangeNotifier
    implements SharedEngineDetachedTabBackend, WindowObserver {
  SharedEngineDetachedTabWindows({
    this.experimentalTerminalTabs = false,
    Future<int> Function(Widget Function(BuildContext, int), WindowOptions)?
    openNativeWindow,
    void Function(int viewId)? completeNativeShow,
    Future<void> Function()? sourceFrameBarrier,
    Future<bool> Function(int)? closeNativeWindow,
    Future<bool> Function()? isCursorOverMainWindow,
    Rect? Function(int viewId)? readPhysicalBounds,
    bool Function(int viewId, Rect bounds)? writePhysicalBounds,
    Duration physicalBoundsPollInterval = const Duration(milliseconds: 250),
  }) : _physicalBoundsPollInterval = physicalBoundsPollInterval,
       _openNativeWindow =
           openNativeWindow ??
           ((builder, options) => openWindow(builder, options: options)),
       _completeNativeShow = completeNativeShow ?? _defaultCompleteShow,
       _sourceFrameBarrier =
           sourceFrameBarrier ?? (() => WidgetsBinding.instance.endOfFrame),
       _closeNativeWindow =
           closeNativeWindow ??
           ((viewId) => MultiViewDesktop.fromId(viewId).closeWindow()),
       _isCursorOverMainWindow =
           isCursorOverMainWindow ?? _nativeCursorOverMainWindow,
       _readPhysicalBounds = readPhysicalBounds ?? _defaultReadPhysicalBounds,
       _writePhysicalBounds =
           writePhysicalBounds ?? _defaultWritePhysicalBounds;

  final Future<int> Function(Widget Function(BuildContext, int), WindowOptions)
  _openNativeWindow;
  final void Function(int viewId) _completeNativeShow;
  final bool experimentalTerminalTabs;
  final Future<void> Function() _sourceFrameBarrier;
  final Future<bool> Function(int) _closeNativeWindow;
  final Future<bool> Function() _isCursorOverMainWindow;
  final Rect? Function(int viewId) _readPhysicalBounds;
  final bool Function(int viewId, Rect bounds) _writePhysicalBounds;
  final Duration _physicalBoundsPollInterval;

  // Last physical frame per tab. Logical coordinates do not round-trip across
  // 120 and 192 DPI, so a reopened child is placed with setPhysicalBounds.
  final Map<String, Rect> _physicalBoundsByTab = <String, Rect>{};
  Timer? _physicalBoundsPoll;

  static Rect? _defaultReadPhysicalBounds(int viewId) {
    try {
      return MultiViewDesktop.fromId(viewId).getPhysicalBounds();
    } catch (error, stackTrace) {
      Logger('SharedEngineDetachedTabWindows')
          .warning('Could not read detached window bounds', error, stackTrace);
      return null;
    }
  }

  static bool _defaultWritePhysicalBounds(int viewId, Rect bounds) {
    try {
      return MultiViewDesktop.fromId(viewId).setPhysicalBounds(bounds);
    } catch (error, stackTrace) {
      Logger(
        'SharedEngineDetachedTabWindows',
      ).warning('Could not restore detached window bounds', error, stackTrace);
      return false;
    }
  }

  /// Releases a window opened with `showOnInit: false`. The view may already
  /// be gone on failure paths, so this must never throw.
  static void _defaultCompleteShow(int viewId) {
    try {
      MultiViewDesktop.fromId(viewId).completeShow();
    } catch (error, stackTrace) {
      Logger('SharedEngineDetachedTabWindows').warning(
        'Could not release the pending detached window show',
        error,
        stackTrace,
      );
    }
  }

  // Registered only by the primary Win32 runner. Its WindowFromPoint result
  // is false if another app *or the detached child* is covering the main HWND.
  // Never infer this from Flutter's drag end offset: coordinates may be over
  // an occluded area or a different monitor's scale.
  static Future<bool> _nativeCursorOverMainWindow() async {
    try {
      return await const MethodChannel('dev.leynier.alera/detached_tabs')
              .invokeMethod<bool>('isCursorOverWindow') ??
          false;
    } on MissingPluginException {
      return false;
    } on PlatformException {
      return false;
    }
  }

  final Map<String, _SharedTabWindow> _active = <String, _SharedTabWindow>{};
  final Set<String> _pending = <String>{};
  bool _disposed = false;

  @override
  Set<String> get detachedTabIds => Set.unmodifiable(_active.keys);

  @override
  bool isDetached(String tabId) => _active.containsKey(tabId);

  @override
  bool isLaunching(String tabId) => _pending.contains(tabId);

  @override
  bool supports(DetachedTabDescriptor descriptor) =>
      Platform.isWindows &&
      (descriptor.tab.kind == WorkspaceTabKind.editor ||
          (experimentalTerminalTabs &&
              descriptor.tab.kind == WorkspaceTabKind.terminal));

  bool canRender(String tabId, int viewId) {
    final entry = _active[tabId];
    return entry != null &&
        entry.viewId == viewId &&
        !entry.nativeClosed &&
        entry.contentMayMount;
  }

  @override
  Future<bool> detach(DetachedTabDescriptor descriptor) async {
    if (_disposed || !supports(descriptor)) return false;
    final id = descriptor.tab.id;
    if (_active.containsKey(id) || !_pending.add(id)) return false;
    notifyListeners();

    _SharedTabWindow? entry;
    try {
      // The native window stays hidden until its text input client exists:
      // platform focus delivers keystrokes to a view before its client is
      // attached, and those characters are dropped without a trace. The view
      // is created with showOnInit: false so openWindow's future hangs on
      // completeShow; the public view id is captured from the builder, which
      // runs on the hidden view's first frame.
      final remembered = _physicalBoundsByTab[id];
      final windowIdCompleter = Completer<int>();
      unawaited(() async {
        try {
          final realId = await _openNativeWindow(
            (context, nativeId) {
              if (!windowIdCompleter.isCompleted) {
                windowIdCompleter.complete(nativeId);
              }
              return _SharedDetachedTabWindow(
                descriptor: descriptor,
                viewId: nativeId,
                controller: this,
              );
            },
            WindowOptions(
              title: 'Alera - ${descriptor.tab.title}',
              size: const Size(1050, 720),
              // Null keeps a saved frame. The package merges an omitted
              // alignment back to Alignment.center, which is only wanted
              // for the first open.
              alignment: remembered == null ? Alignment.center : null,
              showOnInit: false,
            ),
          );
          if (!windowIdCompleter.isCompleted) {
            windowIdCompleter.complete(realId);
          }
        } catch (error, stack) {
          if (!windowIdCompleter.isCompleted) {
            windowIdCompleter.completeError(error, stack);
          } else {
            Logger('SharedEngineDetachedTabWindows').warning(
              'Detached window open finished with an error',
              error,
              stack,
            );
          }
        }
      }());
      final windowId = await windowIdCompleter.future.timeout(
        const Duration(seconds: 15),
      );
      if (remembered != null) {
        _writePhysicalBounds(windowId, remembered);
      }
      entry = _SharedTabWindow(descriptor, windowId);
      if (_disposed || !_pending.contains(id)) {
        _completeNativeShow(windowId);
        entry.showCompleted = true;
        await _closeNativeWindow(windowId);
        return false;
      }
      _active[id] = entry;
      // After setPhysicalBounds. An earlier sample would store the creation
      // frame over the rect just written.
      _ensurePhysicalBoundsPoll();
      notifyListeners(); // Workbench hides and unmounts its source tab.
      await _sourceFrameBarrier();
      if (_disposed || entry.nativeClosed || !identical(_active[id], entry)) {
        return false; // OS window closed while source was unmounting.
      }
      entry.contentMayMount = true;
      notifyListeners(); // Only now may the child attach its tab surface.
      await entry.contentMounted.future.timeout(const Duration(seconds: 15));
      final ready =
          identical(_active[id], entry) && !entry.nativeClosed && !_disposed;
      if (ready) {
        // EarlyInputAttachScope queued setClient during the mount frame while
        // the window was still hidden; showing now means the platform client
        // is registered before any keystroke can be delivered.
        _completeNativeShow(windowId);
        entry.showCompleted = true;
      }
      return ready;
    } catch (error, stack) {
      Logger('SharedEngineDetachedTabWindows').warning(
        'Failed to detach ${descriptor.tab.kind.name} tab ${descriptor.tab.id}',
        error,
        stack,
      );
      if (entry != null && identical(_active[id], entry)) {
        _active.remove(id);
        _stopPhysicalBoundsPollIfIdle();
        notifyListeners();
        // Release the pending showOnInit waiter before tearing the view down,
        // otherwise the openWindow future stays blocked forever.
        try {
          _completeNativeShow(entry.viewId);
          entry.showCompleted = true;
        } catch (_) {}
        try {
          await _closeNativeWindow(entry.viewId);
        } catch (_) {}
      }
      rethrow;
    } finally {
      _pending.remove(id);
      if (!_disposed) notifyListeners();
    }
  }

  void reportContentMounted(String tabId, int viewId) {
    final entry = _active[tabId];
    if (entry == null ||
        entry.viewId != viewId ||
        !entry.contentMayMount ||
        entry.contentMounted.isCompleted) {
      return;
    }
    entry.contentMounted.complete();
  }

  Future<bool> closeTabWindow(String tabId, int viewId) async {
    final entry = _active[tabId];
    if (entry == null || entry.viewId != viewId) return false;
    _capturePhysicalBounds(viewId);
    if (!entry.showCompleted) {
      // The pending showOnInit waiter must be released before the view is
      // destroyed or the openWindow future is leaked.
      _completeNativeShow(viewId);
      entry.showCompleted = true;
    }
    return _closeNativeWindow(viewId);
  }

  /// Dock on native drag end only when the OS pointer is really over Alera's
  /// main HWND. The child must stay open for an unrelated application's HWND.
  /// The identity check after awaiting the probe prevents an obsolete drag
  /// from closing a newly opened window that happens to reuse the view id.
  Future<bool> tryReturnTabAtPointer(String tabId, int viewId) async {
    final entry = _active[tabId];
    if (_disposed ||
        entry == null ||
        entry.viewId != viewId ||
        entry.nativeClosed ||
        !entry.contentMayMount ||
        entry.dockInProgress) {
      return false;
    }
    entry.dockInProgress = true;
    try {
      final allowed = await _isCursorOverMainWindow();
      if (!allowed ||
          _disposed ||
          !identical(_active[tabId], entry) ||
          entry.nativeClosed) {
        return false;
      }
      _capturePhysicalBounds(viewId);
      if (!entry.showCompleted) {
        _completeNativeShow(viewId);
        entry.showCompleted = true;
      }
      return await _closeNativeWindow(viewId);
    } catch (error, stack) {
      Logger('SharedEngineDetachedTabWindows')
          .warning('Native drag-back failed for $tabId', error, stack);
      return false;
    } finally {
      entry.dockInProgress = false;
    }
  }

  @override
  void onWindowOpened(int viewId, {int? parentViewId}) {}

  @override
  void onDialogOpened(int dialogId, {required int parentViewId}) {}

  @override
  void onDialogClose(int dialogId) {}

  @override
  void onAnchorChanged(int? previousViewId, int? newViewId) {}

  @override
  void onWindowEvent(int viewId, String eventName) {
    // "move" and "resize" fire while the drag is in progress. "moved" is
    // skipped when the top-level proc consumes WM_MOVING, and a normal
    // WM_CLOSE never emits "close". The poll covers that gap.
    if (eventName == 'move' ||
        eventName == 'moved' ||
        eventName == 'resize' ||
        eventName == 'resized' ||
        eventName == 'close') {
      _capturePhysicalBounds(viewId);
    }
  }

  void _ensurePhysicalBoundsPoll() {
    if (_disposed || _physicalBoundsPoll != null || _active.isEmpty) return;
    if (_physicalBoundsPollInterval <= Duration.zero) return;
    _physicalBoundsPoll = Timer.periodic(_physicalBoundsPollInterval, (_) {
      _pollOpenPhysicalBounds();
    });
  }

  void _pollOpenPhysicalBounds() {
    if (_disposed) return;
    if (_active.isEmpty) {
      _stopPhysicalBoundsPollIfIdle();
      return;
    }
    for (final entry in _active.values.toList()) {
      if (entry.nativeClosed) continue;
      _capturePhysicalBounds(entry.viewId);
    }
  }

  void _stopPhysicalBoundsPollIfIdle() {
    if (_active.isNotEmpty) return;
    _physicalBoundsPoll?.cancel();
    _physicalBoundsPoll = null;
  }

  void _capturePhysicalBounds(int viewId) {
    final tabId = _tabIdForView(viewId);
    if (tabId == null) return;
    final bounds = _readPhysicalBounds(viewId);
    if (bounds == null ||
        !bounds.left.isFinite ||
        !bounds.top.isFinite ||
        !bounds.width.isFinite ||
        !bounds.height.isFinite ||
        bounds.width <= 0 ||
        bounds.height <= 0) {
      return;
    }
    _physicalBoundsByTab[tabId] = bounds;
  }

  String? _tabIdForView(int viewId) {
    for (final entry in _active.entries) {
      if (entry.value.viewId == viewId) return entry.key;
    }
    return null;
  }

  @override
  void onDialogEvent(int viewId, String eventName) {}

  @override
  void onWindowClosed(int viewId) {
    // The native window is already destroyed here, so the read is often
    // empty. A positive rect still wins; an empty one keeps the last sample.
    _capturePhysicalBounds(viewId);
    final tabId = _active.entries
        .where((e) => e.value.viewId == viewId)
        .map((e) => e.key)
        .firstOrNull;
    if (tabId == null) return;
    final entry = _active[tabId]!;
    if (entry.nativeClosed) return;
    entry.nativeClosed = true;
    if (!entry.contentMounted.isCompleted) {
      // A close may arrive *before* detach() awaits this completer at the
      // source-unmount barrier. Completing with an error at that point emits
      // an unhandled zone exception. Wake the waiter normally and check the
      // removed entry to report a failed handoff without losing the tab.
      entry.contentMounted.complete();
    }
    // The child must unmount before the source remounts: Editor materializes
    // its dirty CodeForge buffer into the shared EditorSessionRegistry;
    // Terminal releases its view/visibility attachment without killing PTY.
    // Releasing the active tab immediately can mount both tab views at once.
    WidgetsBinding.instance.scheduleFrame();
    unawaited(_restoreSourceAfterNativeClose(tabId, entry));
  }

  Future<void> _restoreSourceAfterNativeClose(
    String tabId,
    _SharedTabWindow entry,
  ) async {
    try {
      await _sourceFrameBarrier();
    } catch (error, stack) {
      Logger('SharedEngineDetachedTabWindows')
          .warning('Child close frame barrier failed for $tabId', error, stack);
    }
    if (_disposed || !identical(_active[tabId], entry)) return;
    _active.remove(tabId);
    _stopPhysicalBoundsPollIfIdle();
    // The main tab remounts only after the child's teardown frame.
    notifyListeners();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _physicalBoundsPoll?.cancel();
    _physicalBoundsPoll = null;
    for (final entry in _active.values) {
      if (!entry.contentMounted.isCompleted) {
        entry.contentMounted.complete();
      }
    }
    _active.clear();
    _pending.clear();
    _physicalBoundsByTab.clear();
    super.dispose();
  }
}

final class _SharedTabWindow {
  _SharedTabWindow(this.descriptor, this.viewId);

  final DetachedTabDescriptor descriptor;
  final int viewId;
  final Completer<void> contentMounted = Completer<void>();
  bool contentMayMount = false;
  bool nativeClosed = false;
  bool dockInProgress = false;
  bool showCompleted = false;
}
