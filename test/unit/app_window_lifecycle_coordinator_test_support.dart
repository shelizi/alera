part of 'app_window_lifecycle_coordinator_test.dart';

class _RecordingStateRepository implements AppWindowStateRepository {
  AppWindowState? state;
  Object? saveError;
  Completer<void>? saveStarted;
  Future<void>? saveBarrier;
  Completer<void>? loadStarted;
  Future<AppWindowState?>? loadOverride;
  Object? loadError;
  final List<AppWindowState> saved = <AppWindowState>[];

  @override
  Future<void> clear() async {
    state = null;
  }

  @override
  Future<AppWindowState?> load() async {
    final override = loadOverride;
    if (override != null) {
      loadOverride = null;
      final started = loadStarted;
      loadStarted = null;
      if (started != null && !started.isCompleted) {
        started.complete();
      }
      return override;
    }
    final error = loadError;
    if (error != null) {
      throw error;
    }
    return state;
  }

  @override
  Future<void> save(AppWindowState state) async {
    saveStarted?.complete();
    await saveBarrier;
    final error = saveError;
    if (error != null) {
      throw error;
    }
    this.state = state;
    saved.add(state);
  }
}

class _RecordingWindowController implements AppWindowController {
  final List<AppWindowEventListener> listeners = <AppWindowEventListener>[];
  final List<bool> preventCloseValues = <bool>[];
  Rect bounds = const .fromLTWH(20, 30, 800, 500);
  int destroyCalls = 0;
  int hideCalls = 0;
  int showCalls = 0;
  int focusCalls = 0;
  int restoreCalls = 0;
  bool visible = true;
  bool minimized = false;
  int captureCalls = 0;
  Completer<void>? captureStarted;
  Future<void>? captureBarrier;
  Completer<void>? hideStarted;
  Future<void>? hideBarrier;
  Completer<void>? enablePreventCloseStarted;
  Future<void>? enablePreventCloseBarrier;
  bool preventClose = false;

  void emit(void Function(AppWindowEventListener listener) notify) {
    for (final listener in List<AppWindowEventListener>.from(listeners)) {
      notify(listener);
    }
  }

  @override
  void addListener(AppWindowEventListener listener) {
    listeners.add(listener);
  }

  @override
  Future<void> close() async {
    emit((listener) => listener.onWindowClose());
  }

  @override
  Future<void> destroy() async {
    destroyCalls += 1;
  }

  @override
  Future<void> hide() async {
    hideStarted?.complete();
    await hideBarrier;
    hideCalls += 1;
    visible = false;
  }

  @override
  Future<void> show() async {
    showCalls += 1;
    visible = true;
  }

  @override
  Future<void> restore() async {
    restoreCalls += 1;
    minimized = false;
  }

  @override
  Future<void> focus() async {
    focusCalls += 1;
  }

  @override
  Future<bool> isVisible() async => visible;

  @override
  Future<Rect> getBounds() async => bounds;

  @override
  Future<bool> isFullScreen() async => false;

  @override
  Future<bool> isMaximized() async => false;

  @override
  Future<bool> isMinimized() async {
    captureCalls += 1;
    final started = captureStarted;
    if (started != null && !started.isCompleted) {
      started.complete();
    }
    await captureBarrier;
    return minimized;
  }

  @override
  Future<void> maximize() async {}

  @override
  void removeListener(AppWindowEventListener listener) {
    listeners.remove(listener);
  }

  @override
  Future<void> setBounds(Rect bounds) async {
    this.bounds = bounds;
  }

  @override
  Future<void> setFullScreen(bool value) async {}

  @override
  Future<void> setPreventClose(bool value) async {
    preventCloseValues.add(value);
    if (value) {
      final started = enablePreventCloseStarted;
      if (started != null && !started.isCompleted) {
        started.complete();
      }
      await enablePreventCloseBarrier;
    }
    preventClose = value;
  }

  @override
  Future<void> setTitle(String title) async {}
}

Future<void> _waitFor(bool Function() predicate) async {
  final deadline = DateTime.now().add(const Duration(seconds: 2));
  while (!predicate()) {
    if (DateTime.now().isAfter(deadline)) {
      throw TimeoutException('condition was not reached');
    }
    await Future.pause(.zero);
  }
}
