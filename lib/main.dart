import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/app.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/application/detached_tab_bootstrap.dart';
import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
import 'package:alera/src/features/workbench/presentation/shared_engine_detached_tab_window.dart';
import 'package:window_manager/window_manager.dart';
import 'package:alera/src/app/resource_manager_terminal_composition.dart';
import 'package:alera/src/app/runtime_host_window_exit_composition.dart';
import 'package:alera/src/app/terminal_runtime_composition.dart';
import 'package:alera/src/features/app_window/infra/app_window_bootstrap.dart';
import 'package:alera/src/rust/frb_generated.dart';
import 'package:alera/src/shared/infra/logging/app_logger.dart';
import 'package:alera/src/shared/infra/logging/global_error_handlers.dart';
import 'package:alera/src/shared/infra/logging/logging_provider_observer.dart';
import 'package:alera/src/shared/infra/performance/performance_trace.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:ghostty_vte_flutter/ghostty_vte_flutter.dart';
import 'package:logging/logging.dart';
import 'package:multiview_desktop/multiview_desktop.dart';

/// Deliberately command-line only. The normal Alera launch always retains
/// the established per-process detached tab implementation.
const experimentalSharedEngineTabsFlag = '--alera-shared-engine-tabs';

bool isExperimentalSharedEngineTabsInvocation(List<String> args) =>
    args.length == 1 && args.single == experimentalSharedEngineTabsFlag;

Future<void> main(List<String> args) async {
  await runZonedGuarded<Future<void>>(() => _bootstrap(args), recordZoneError);
}

Future<void> _bootstrap(List<String> args) async {
  AleraPerformanceTrace.startStartup();
  WidgetsFlutterBinding.ensureInitialized();
  AleraPerformanceTrace.mark('widgets_initialized');

  final detachedArgs = DetachedTabLaunchArguments.parse(args);
  if (detachedArgs != null) {
    // Connect before Rust/SQLite initialization: the native HWND used to
    // remain invisible until *all* of that work finished. Flutter can paint
    // a small initial scene immediately and finish native initialization
    // while the user sees which tab is opening.
    final client = await DetachedTabWindowClient.connect(detachedArgs);
    // The Win32 runner already has a usable HWND and temporary Alera title.
    // Defer plugin window setup until after the first visible Flutter frame.
    AleraPerformanceTrace.recordFirstFrame();
    runApp(DetachedTabStartupApp(title: client.descriptor.tab.title));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      AleraPerformanceTrace.mark('detached_preview_first_frame');
      client.loading();
      unawaited(_finishDetachedTabInitialization(client));
    });
    return;
  }
  // Logging comes up before anything that can fail. It used to run after both
  // Rust initializations, so a failure in either went to a console a packaged
  // build does not have, and was lost.
  await AppLogger.configure();
  installGlobalErrorHandlers();

  // Privacy portable build: do not initialize the Sentry SDK at all.
  await _startApp(
    sharedEngineTabs:
        Platform.isWindows && isExperimentalSharedEngineTabsInvocation(args),
  );
}

Future<void> _finishDetachedTabInitialization(
  DetachedTabWindowClient client,
) async {
  AleraDatabase? childDatabase;
  final elapsed = Stopwatch()..start();
  try {
    installGlobalErrorHandlers();
    // File-backed logging, window plugin setup, both independent Rust
    // engines and SQLite now start together, without serializing the cold
    // Rust/DB path behind a platform directory lookup for the log sink.
    childDatabase = await initializeDetachedTabDependencies<AleraDatabase>(
      kind: client.descriptor.tab.kind,
      configureLogging: () => AleraPerformanceTrace.measureAsync(
        'detached_logger_init',
        AppLogger.configure,
      ),
      initializeWindow: () =>
          AleraPerformanceTrace.measureAsync('detached_window_setup', () async {
            await windowManager.ensureInitialized();
            await windowManager.setTitle(
              'Alera - ' + client.descriptor.tab.title,
            );
          }),
      initializeAleraRust: () => AleraPerformanceTrace.measureAsync(
        'detached_alera_rust_init',
        RustLib.init,
      ),
      initializeCodeForge: () => AleraPerformanceTrace.measureAsync(
        'detached_code_forge_rust_init',
        code_forge.RustLib.init,
      ),
      openDatabase: () => AleraPerformanceTrace.measureAsync(
        'detached_database_open',
        openAleraDb,
      ),
      closeDatabase: (database) => database.close(),
    );
    // Keep the synchronous provider contract for the actual editor widget.
    // Only the lightweight initial frame is rendered without the database.
    final readyDatabase = childDatabase;
    await windowManager.setPreventClose(true);
    Logger('DetachedTabStartup')
        .info('Native tab initialized in ${elapsed.elapsedMilliseconds}ms');
    runApp(
      ProviderScope(
        observers: const <ProviderObserver>[LoggingProviderObserver()],
        overrides: [
          resourceManagerTerminalBindingOverride(),
          terminalRuntimeBindingOverride(),
          runtimeHostWindowExitOverride(),
          aleraDatabaseProvider.overrideWith((ref) {
            ref.onDispose(() async {
              await readyDatabase.close();
            });
            return readyDatabase;
          }),
        ],
        child: DetachedTabChildApp(client: client),
      ),
    );
    WidgetsBinding.instance.addPostFrameCallback((_) {
      AleraPerformanceTrace.mark('detached_content_first_frame');
      client.ready();
    });
  } catch (error, stackTrace) {
    Logger('DetachedTabStartup').warning(
      'Failed after ${elapsed.elapsedMilliseconds}ms',
      error,
      stackTrace,
    );
    try {
      client.failed();
    } catch (_) {
      // The parent may already have closed the launch socket.
    }
    // The coordinator already closes a DB opened during a failed parallel
    // task. This only covers failures after a fully successful bootstrap
    // (for example setPreventClose or building the actual app).
    await childDatabase?.close();
    runApp(
      DetachedTabStartupApp(title: client.descriptor.tab.title, failed: true),
    );
  }
}

Future<void> _startApp({bool sharedEngineTabs = false}) async {
  await RustLib.init();
  AleraPerformanceTrace.mark('alera_rust_initialized');
  await code_forge.RustLib.init();
  AleraPerformanceTrace.mark('code_forge_rust_initialized');
  final windowBootstrap = await bootstrapAppWindowBeforeRunApp();
  AleraPerformanceTrace.mark('window_bootstrapped');
  await initializeGhosttyVteWeb();
  AleraPerformanceTrace.mark('ghostty_initialized');
  AleraPerformanceTrace.recordFirstFrame();

  Widget appScope(Widget child) => ProviderScope(
    observers: const <ProviderObserver>[LoggingProviderObserver()],
    overrides: [
      resourceManagerTerminalBindingOverride(),
      terminalRuntimeBindingOverride(),
      runtimeHostWindowExitOverride(),
      if (windowBootstrap.database case final db?)
        aleraDatabaseProvider.overrideWith((ref) {
          ref.onDispose(() async {
            await db.close();
          });
          // The desktop bootstrap already opened the database. Returning it
          // synchronously keeps dependent providers out of AsyncLoading on
          // their first build while preserving the FutureProvider contract.
          return db;
        }),
    ],
    child: child,
  );

  if (sharedEngineTabs) {
    // All Flutter Views must share the *same* root ProviderScope. Creating a
    // ProviderScope per child would silently duplicate EditorSessionRegistry,
    // Rust-backed runtime state and the SQLite owner.
    final windows = SharedEngineDetachedTabWindows();
    detachedTabWindows.useExperimentalSharedBackend(windows);
    runMultiApp(
      home: (_, _) => const AleraApp(),
      globalScope: appScope,
      config: MultiAppConfig(
        observers: <WindowObserver>[windows],
        generalParams: MultiPlatformParams(
          closeMode: CloseMode.softCascade,
          animation: ViewAnimationConfig.disabled,
        ),
      ),
    );
  } else {
    runApp(appScope(const AleraApp()));
  }
  AleraPerformanceTrace.mark('run_app_called');
}
