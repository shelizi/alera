import 'package:alera/src/features/app_window/application/app_window_controller.dart';
import 'package:alera/src/features/app_window/application/app_window_state_repository.dart';
import 'package:alera/src/features/app_window/domain/app_foreground.dart';
import 'package:alera/src/features/app_window/infra/drift_app_window_state_repository.dart';
import 'package:alera/src/features/app_window/infra/lifecycle_app_foreground.dart';
import 'package:alera/src/features/app_window/infra/platform_app_window_close_strategy.dart';
import 'package:alera/src/features/app_window/infra/screen_retriever_app_window_display_provider.dart';
import 'package:alera/src/features/app_window/infra/window_manager_app_window_controller.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart' show Provider;
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'app_window_providers.g.dart';

typedef AppWindowBeforeLinuxExit = Future<void> Function();

final appWindowBeforeLinuxExitProvider = Provider<AppWindowBeforeLinuxExit?>(
  (ref) => null,
);

@Riverpod(keepAlive: true)
AppWindowStateRepository appWindowStateRepository(Ref ref) {
  final db = ref.watch(aleraDatabaseProvider).requireValue;
  return DriftAppWindowStateRepository(db);
}

@Riverpod(keepAlive: true)
AppWindowController appWindowController(Ref ref) {
  return WindowManagerAppWindowController();
}

@Riverpod(keepAlive: true)
AppWindowDisplayProvider appWindowDisplayProvider(Ref ref) {
  return ScreenRetrieverAppWindowDisplayProvider();
}

@Riverpod(keepAlive: true)
AppWindowLifecycleCoordinator appWindowLifecycleCoordinator(Ref ref) {
  final coordinator = AppWindowLifecycleCoordinator(
    repository: ref.watch(appWindowStateRepositoryProvider),
    window: ref.watch(appWindowControllerProvider),
    closeStrategy: PlatformAppWindowCloseStrategy(
      beforeLinuxExit: ref.watch(appWindowBeforeLinuxExitProvider),
    ),
  );
  ref.onDispose(() {
    coordinator.stop();
  });
  return coordinator;
}

/// Observes the app lifecycle so recurring work can park while nobody can see
/// its results.
@Riverpod(keepAlive: true)
AppForeground appForeground(Ref ref) {
  final foreground = LifecycleAppForeground();
  ref.onDispose(foreground.dispose);
  return foreground;
}
