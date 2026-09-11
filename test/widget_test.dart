import 'package:alera/src/app/app.dart';
import 'package:alera/src/app/resource_manager_terminal_composition.dart';
import 'package:alera/src/app/runtime_host_window_exit_composition.dart';
import 'package:alera/src/app/terminal_runtime_composition.dart';
import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:alera/src/features/runtime_host/presentation/runtime_host_quit_gate_scope.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:drift/native.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('renders alera shell', (tester) async {
    final db = await _pumpTestApp(tester);
    try {
      expect(find.byType(AleraShellPage), findsOneWidget);
    } finally {
      await _disposeTestApp(tester, db);
    }
  });

  testWidgets('places the runtime quit gate below the navigator', (
    tester,
  ) async {
    final db = await _pumpTestApp(tester);
    try {
      expect(
        find.ancestor(
          of: find.byType(RuntimeHostQuitGateScope),
          matching: find.byType(Navigator),
        ),
        findsOneWidget,
      );
    } finally {
      await _disposeTestApp(tester, db);
    }
  });
}

Future<AleraDatabase> _pumpTestApp(WidgetTester tester) async {
  final db = await openAleraDb(executor: NativeDatabase.memory());
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        resourceManagerTerminalBindingOverride(),
        terminalRuntimeBindingOverride(),
        runtimeHostWindowExitOverride(),
        aleraDatabaseProvider.overrideWith((ref) => db),
      ],
      child: const AleraApp(),
    ),
  );
  await tester.pump(const Duration(seconds: 1));
  return db;
}

Future<void> _disposeTestApp(WidgetTester tester, AleraDatabase db) async {
  await tester.pumpWidget(const SizedBox.shrink());
  await tester.pump();
  await db.close();
}
