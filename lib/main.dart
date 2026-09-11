import 'dart:async';

import 'package:alera/src/app/app.dart';
import 'package:alera/src/app/runtime_host_window_exit_composition.dart';
import 'package:alera/src/app/terminal_runtime_composition.dart';
import 'package:alera/src/features/app_window/infra/app_window_bootstrap.dart';
import 'package:alera/src/rust/frb_generated.dart';
import 'package:alera/src/shared/infra/logging/app_logger.dart';
import 'package:alera/src/shared/infra/logging/global_error_handlers.dart';
import 'package:alera/src/shared/infra/logging/logging_provider_observer.dart';
import 'package:alera/src/shared/infra/performance/performance_trace.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:ghostty_vte_flutter/ghostty_vte_flutter.dart';

Future<void> main(List<String> args) async {
  await runZonedGuarded<Future<void>>(_bootstrap, recordZoneError);
}

Future<void> _bootstrap() async {
  AleraPerformanceTrace.startStartup();
  WidgetsFlutterBinding.ensureInitialized();
  AleraPerformanceTrace.mark('widgets_initialized');

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
