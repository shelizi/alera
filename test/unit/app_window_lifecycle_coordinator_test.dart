import 'dart:async';
import 'dart:ui';

import 'package:alera/src/features/app_window/application/app_window_controller.dart';
import 'package:alera/src/features/app_window/application/app_window_state_repository.dart';
import 'package:alera/src/features/app_window/domain/app_window_state.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:logging/logging.dart';

part 'app_window_lifecycle_coordinator_test_support.dart';

void main() {
  group('AppWindowLifecycleCoordinator shutdown', () {
    test('stop wins over an in-flight start prevent-close enable', () async {
      final enableStarted = Completer<void>();
      final enableRelease = Completer<void>();
      final window = _RecordingWindowController()
        ..enablePreventCloseStarted = enableStarted
        ..enablePreventCloseBarrier = enableRelease.future;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: _RecordingStateRepository(),
        window: window,
        saveDebounce: .zero,
      );

      final starting = coordinator.start();
      await enableStarted.future;
      await coordinator.stop();
      expect(window.preventClose, isFalse);

      enableRelease.complete();
      await starting;

      expect(window.listeners, isEmpty);
      expect(window.preventClose, isFalse);
    });

    test('failed start rolls back listener and prevent-close state', () async {
      final repository = _RecordingStateRepository()
        ..loadError = StateError('load failed');
      final window = _RecordingWindowController();
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
      );

      await expectLater(coordinator.start(), throwsStateError);

      expect(window.listeners, isEmpty);
      expect(window.preventClose, isFalse);
      expect(window.preventCloseValues, <bool>[true, false]);

      repository.loadError = null;
      await coordinator.start();
      expect(window.listeners, <AppWindowEventListener>[coordinator]);
      expect(window.preventClose, isTrue);
    });

    test(
      'restart ignores state restored by an older in-flight start',
      () async {
        final firstLoadStarted = Completer<void>();
        final firstLoad = Completer<AppWindowState?>();
        final repository = _RecordingStateRepository()
          ..loadStarted = firstLoadStarted
          ..loadOverride = firstLoad.future;
        final window = _RecordingWindowController();
        final coordinator = AppWindowLifecycleCoordinator(
          repository: repository,
          window: window,
          saveDebounce: .zero,
        );

        final firstStart = coordinator.start();
        await firstLoadStarted.future;
        await coordinator.stop();
        repository.state = AppWindowState(
          normalBounds: AppWindowBounds.fromRect(window.bounds),
        );
        await coordinator.start();

        firstLoad.complete(
          const AppWindowState(
            normalBounds: AppWindowBounds(
              left: 1,
              top: 2,
              width: 300,
              height: 200,
            ),
          ),
        );
        await firstStart;
        await coordinator.flush();

        expect(repository.saved, isEmpty);
        expect(window.preventClose, isTrue);
        expect(window.listeners, <AppWindowEventListener>[coordinator]);
      },
    );

    test('resets close state before a gate failure is logged', () async {
      final repository = _RecordingStateRepository();
      final window = _RecordingWindowController();
      final logger = Logger.detached('close-gate-ordering');
      var gateCalls = 0;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        logger: logger,
        closeGate: () async {
          gateCalls += 1;
          if (gateCalls == 1) {
            throw StateError('quit gate failed');
          }
          return true;
        },
      );
      final subscription = logger.onRecord.listen((_) {
        window.emit((listener) => listener.onWindowClose());
      });
      addTearDown(subscription.cancel);
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      await _waitFor(() => window.destroyCalls == 1);

      expect(gateCalls, 2);
      expect(window.destroyCalls, 1);
      expect(window.preventCloseValues, <bool>[true, false]);
    });

    test('does not publish warnings during committed close', () async {
      final repository = _RecordingStateRepository()
        ..saveError = StateError('disk full');
      final window = _RecordingWindowController();
      final logger = Logger.detached('committed-close');
      final records = <LogRecord>[];
      final subscription = logger.onRecord.listen(records.add);
      addTearDown(subscription.cancel);
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        logger: logger,
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      await _waitFor(() => window.destroyCalls == 1);

      expect(records, isEmpty);
      expect(window.destroyCalls, 1);
      expect(window.preventCloseValues, <bool>[true, false]);
    });

    test('reports save failures while the close gate is pending', () async {
      final saveStarted = Completer<void>();
      final finishSave = Completer<void>();
      final gate = Completer<bool>();
      final repository = _RecordingStateRepository()
        ..saveStarted = saveStarted
        ..saveBarrier = finishSave.future
        ..saveError = StateError('disk full');
      final window = _RecordingWindowController();
      final logger = Logger.detached('pending-close');
      final records = <LogRecord>[];
      final subscription = logger.onRecord.listen(records.add);
      addTearDown(subscription.cancel);
      var gateCalls = 0;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        logger: logger,
        closeGate: () {
          gateCalls += 1;
          return gate.future;
        },
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowResized());
      await saveStarted.future;
      window.emit((listener) => listener.onWindowClose());
      finishSave.complete();
      await _waitFor(() => records.isNotEmpty);
      gate.complete(false);
      await Future.pause(.zero);

      expect(records.single.message, 'failed to save app window state');
      expect(window.destroyCalls, 0);
      window.emit((listener) => listener.onWindowClose());
      await _waitFor(() => gateCalls == 2);
    });

    test('hides the window when hide-on-close is bound', () async {
      final repository = _RecordingStateRepository();
      final window = _RecordingWindowController();
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        hideOnClose: () => true,
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      await _waitFor(() => window.hideCalls == 1);

      expect(window.destroyCalls, 0);
      expect(window.hideCalls, 1);
      expect(window.preventCloseValues, <bool>[true]);
    });

    test('hide-on-close does not wait for a slow window state save', () async {
      final saveStarted = Completer<void>();
      final finishSave = Completer<void>();
      final repository = _RecordingStateRepository()
        ..saveStarted = saveStarted
        ..saveBarrier = finishSave.future;
      final window = _RecordingWindowController();
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        hideOnClose: () => true,
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      await saveStarted.future;
      await _waitFor(() => window.hideCalls == 1);

      expect(window.hideCalls, 1);
      expect(finishSave.isCompleted, isFalse);

      finishSave.complete();
      await coordinator.waitForPendingHide();
    });

    test('requestQuit destroys even when hide-on-close is bound', () async {
      final repository = _RecordingStateRepository();
      final window = _RecordingWindowController();
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        hideOnClose: () => true,
      );
      await coordinator.start();

      await coordinator.requestQuit();
      await _waitFor(() => window.destroyCalls == 1);

      expect(window.hideCalls, 0);
      expect(window.destroyCalls, 1);
      expect(window.preventCloseValues, <bool>[true, false]);
    });

    test(
      'requestQuit waits for an in-flight hide before the close gate',
      () async {
        final saveStarted = Completer<void>();
        final finishSave = Completer<void>();
        final gate = Completer<bool>();
        final repository = _RecordingStateRepository()
          ..saveStarted = saveStarted
          ..saveBarrier = finishSave.future;
        final window = _RecordingWindowController();
        var gateCalls = 0;
        final coordinator = AppWindowLifecycleCoordinator(
          repository: repository,
          window: window,
          saveDebounce: .zero,
          hideOnClose: () => true,
          closeGate: () {
            gateCalls += 1;
            return gate.future;
          },
        );
        await coordinator.start();

        window.emit((listener) => listener.onWindowClose());
        window.emit((listener) => listener.onWindowClose());
        await saveStarted.future;
        expect(window.hideCalls, 0);
        expect(gateCalls, 0);

        final quit = coordinator.requestQuit();
        await Future.pause(.zero);
        expect(gateCalls, 0);
        expect(window.hideCalls, 0);
        expect(window.destroyCalls, 0);

        finishSave.complete();
        await _waitFor(() => gateCalls == 1);
        expect(window.hideCalls, 0);

        gate.complete(false);
        await quit;
        expect(window.destroyCalls, 0);

        window.emit((listener) => listener.onWindowClose());
        await _waitFor(() => window.hideCalls == 1);
        expect(window.destroyCalls, 0);
      },
    );

    test('requestQuit finishes hide before opening the close gate', () async {
      final hideStarted = Completer<void>();
      final finishHide = Completer<void>();
      final gate = Completer<bool>();
      final window = _RecordingWindowController()
        ..hideStarted = hideStarted
        ..hideBarrier = finishHide.future;
      var gateCalls = 0;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: _RecordingStateRepository(),
        window: window,
        saveDebounce: .zero,
        hideOnClose: () => true,
        closeGate: () {
          gateCalls += 1;
          return gate.future;
        },
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      await hideStarted.future;
      expect(gateCalls, 0);

      final quit = coordinator.requestQuit();
      await Future.pause(.zero);
      expect(gateCalls, 0);
      expect(window.destroyCalls, 0);

      finishHide.complete();
      await _waitFor(() => gateCalls == 1);
      expect(window.hideCalls, 1);
      expect(window.visible, isFalse);

      gate.complete(false);
      await quit;
      expect(window.destroyCalls, 0);
    });

    test('coalesces repeated window state saves before quit flush', () async {
      final captureStarted = Completer<void>();
      final releaseCapture = Completer<void>();
      final window = _RecordingWindowController()
        ..captureStarted = captureStarted
        ..captureBarrier = releaseCapture.future;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: _RecordingStateRepository(),
        window: window,
        saveDebounce: .zero,
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowResized());
      await captureStarted.future;
      for (var i = 0; i < 50; i += 1) {
        window.emit((listener) => listener.onWindowResized());
      }

      final quit = coordinator.requestQuit();
      releaseCapture.complete();
      await quit;

      expect(window.captureCalls, 2);
      expect(window.destroyCalls, 1);
    });

    test('requestQuit does not wait for a slow window state save', () async {
      final saveStarted = Completer<void>();
      final finishSave = Completer<void>();
      final repository = _RecordingStateRepository()
        ..saveStarted = saveStarted
        ..saveBarrier = finishSave.future;
      final window = _RecordingWindowController();
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowResized());
      await saveStarted.future;

      final quit = coordinator.requestQuit();
      await _waitFor(() => window.destroyCalls == 1);

      expect(window.destroyCalls, 1);
      expect(finishSave.isCompleted, isFalse);

      finishSave.complete();
      await quit;
    });

    test('closes once and ignores post-close state work', () async {
      final repository = _RecordingStateRepository();
      final window = _RecordingWindowController();
      final gate = Completer<bool>();
      var gateCalls = 0;
      final coordinator = AppWindowLifecycleCoordinator(
        repository: repository,
        window: window,
        saveDebounce: .zero,
        closeGate: () {
          gateCalls += 1;
          return gate.future;
        },
      );
      await coordinator.start();

      window.emit((listener) => listener.onWindowClose());
      window.emit((listener) => listener.onWindowClose());
      gate.complete(true);
      await _waitFor(() => window.destroyCalls == 1);
      final savesAtClose = repository.saved.length;

      window.bounds = const Rect.fromLTWH(100, 120, 900, 600);
      window.emit((listener) => listener.onWindowResized());
      window.emit((listener) => listener.onWindowClose());
      await coordinator.flush();

      expect(gateCalls, 1);
      expect(window.destroyCalls, 1);
      expect(repository.saved, hasLength(savesAtClose));
      expect(window.preventCloseValues, <bool>[true, false]);
    });
  });
}
