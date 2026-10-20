import 'dart:async';

import 'package:alera/src/app/app.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
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

Future<void> main(List<String> args) async {
  await runZonedGuarded<Future<void>>(() => _bootstrap(args), recordZoneError);
}

Future<void> _bootstrap(List<String> args) async {
  AleraPerformanceTrace.startStartup();
  WidgetsFlutterBinding.ensureInitialized();
  AleraPerformanceTrace.mark('widgets_initialized');

  final detachedArgs = DetachedTabLaunchArguments.parse(args);
  if (detachedArgs != null) {
    await AppLogger.configure();
    installGlobalErrorHandlers();
    await RustLib.init();
    await code_forge.RustLib.init();
    await initializeGhosttyVteWeb();
    final client = await DetachedTabWindowClient.connect(detachedArgs);
    // Match the primary window's synchronous bootstrap provider contract.
    // Without this, settingsRepositoryProvider reads AsyncLoading on the
    // child window's first frame, before the database has opened.
    final childDatabase = await openAleraDb();
    await windowManager.ensureInitialized();
    await windowManager.setTitle('Alera - ' + client.descriptor.tab.title);
    await windowManager.setPreventClose(true);
    runApp(
      ProviderScope(
        observers: const <ProviderObserver>[LoggingProviderObserver()],
        overrides: [
          resourceManagerTerminalBindingOverride(),
          terminalRuntimeBindingOverride(),
          runtimeHostWindowExitOverride(),
          aleraDatabaseProvider.overrideWith((ref) {
            ref.onDispose(() async {
              await childDatabase.close();
            });
            return childDatabase;
          }),
        ],
        child: DetachedTabChildApp(client: client),
      ),
    );
    WidgetsBinding.instance.addPostFrameCallback((_) => client.ready());
    return;
  }
  // Logging comes up before anything that can fail. It used to run after both
  // Rust initializations, so a failure in either went to a console a packaged
  // build does not have, and was lost.
  await AppLogger.configure();
  installGlobalErrorHandlers();

  // Privacy portable build: do not initialize the Sentry SDK at all.
  await _startApp();
}

Future<void> _startApp() async {
  await RustLib.init();
  AleraPerformanceTrace.mark('alera_rust_initialized');
  await code_forge.RustLib.init();
  AleraPerformanceTrace.mark('code_forge_rust_initialized');
  final windowBootstrap = await bootstrapAppWindowBeforeRunApp();
  AleraPerformanceTrace.mark('window_bootstrapped');
  await initializeGhosttyVteWeb();
  AleraPerformanceTrace.mark('ghostty_initialized');
  AleraPerformanceTrace.recordFirstFrame();

  runApp(
    ProviderScope(
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
      child: const AleraApp(),
    ),
  );
  AleraPerformanceTrace.mark('run_app_called');
}
