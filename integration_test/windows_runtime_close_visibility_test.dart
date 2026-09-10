import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/app_window/application/app_window_controller.dart';
import 'package:alera/src/features/app_window/application/app_window_state_repository.dart';
import 'package:alera/src/features/app_window/domain/app_window_state.dart';
import 'package:alera/src/features/app_window/infra/window_manager_app_window_controller.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:window_manager/window_manager.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Windows quit cancel keeps the native window visible', (
    tester,
  ) async {
    await tester.pumpWidget(const SizedBox());
    await windowManager.ensureInitialized();
    final window = WindowManagerAppWindowController();
    final lifecycle = AppWindowLifecycleCoordinator(
      repository: _MemoryWindowStateRepository(),
      window: window,
      closeStrategy: const _KeepRunnerAliveCloseStrategy(),
      closeGate: () async => false,
    );
    addTearDown(() async {
      await window.show();
      await lifecycle.stop();
    });

    await lifecycle.start();
    await window.show();
    expect(await window.isVisible(), isTrue);

    await lifecycle.requestQuit();

    expect(lifecycle.isQuitting, isFalse);
    expect(await window.isVisible(), isTrue);
  }, skip: !Platform.isWindows);

  testWidgets(
    'Windows committed quit hides the native window before slow cleanup finishes',
    (tester) async {
      await tester.pumpWidget(const SizedBox());
      await windowManager.ensureInitialized();
      final window = WindowManagerAppWindowController();
      final cleanupMayFinish = Completer<void>();
      var cleanupFinished = false;
      final lifecycle = AppWindowLifecycleCoordinator(
        repository: _MemoryWindowStateRepository(),
        window: window,
        closeStrategy: const _KeepRunnerAliveCloseStrategy(),
        closeGate: () async {
          // Mirrors RuntimeHostQuitGateScope.onBusyQuitCommitted: the user's
          // decision is final, so native visibility must change immediately and
          // must not wait for a slow runtime force-stop/cleanup RPC.
          unawaited(window.hide());
          await cleanupMayFinish.future;
          cleanupFinished = true;
          return true;
        },
      );
      addTearDown(() async {
        if (!cleanupMayFinish.isCompleted) {
          cleanupMayFinish.complete();
        }
        await window.show();
        await lifecycle.stop();
      });

      await lifecycle.start();
      await window.show();
      expect(await window.isVisible(), isTrue);

      final quitFuture = lifecycle.requestQuit();
      final stopwatch = Stopwatch()..start();
      final hidden = await _waitUntilHidden(
        window,
        timeout: const Duration(milliseconds: 750),
      );
      stopwatch.stop();

      expect(
        hidden,
        isTrue,
        reason: 'native Windows window never became hidden',
      );
      expect(
        stopwatch.elapsed,
        lessThan(const Duration(milliseconds: 750)),
        reason: 'native window hide was blocked by runtime cleanup',
      );
      expect(cleanupFinished, isFalse);

      cleanupMayFinish.complete();
      await quitFuture;
    },
    skip: !Platform.isWindows,
  );
}

Future<bool> _waitUntilHidden(
  AppWindowController window, {
  required Duration timeout,
}) async {
  final deadline = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(deadline)) {
    if (!await window.isVisible()) {
      return true;
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  return !await window.isVisible();
}

class _MemoryWindowStateRepository implements AppWindowStateRepository {
  AppWindowState? state;

  @override
  Future<AppWindowState?> load() async => state;

  @override
  Future<void> save(AppWindowState state) async => this.state = state;

  @override
  Future<void> clear() async => state = null;
}

final class _KeepRunnerAliveCloseStrategy implements AppWindowCloseStrategy {
  const _KeepRunnerAliveCloseStrategy();

  @override
  Future<void> close(AppWindowController window) async {
    // A real app quit destroys this HWND. The integration runner must stay
    // alive long enough to report assertions, so the test verifies visibility
    // and intentionally leaves final destruction to the test harness.
    await window.setPreventClose(false);
  }
}
