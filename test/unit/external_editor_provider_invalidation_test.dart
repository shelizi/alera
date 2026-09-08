import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/application/settings_providers.dart';
import 'package:alera/src/features/settings/infra/drift_settings_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator_provider.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:drift/native.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'unrelated settings do not rebuild external editor routing providers',
    () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      final initialLauncher = container.read(externalEditorLauncherProvider);
      final initialCoordinator = container.read(
        workspaceFileOpenCoordinatorProvider,
      );

      await controller.updateEditor(
        (editor) => editor.copyWith(tabSize: editor.tabSize + 1),
      );

      expect(
        identical(
          container.read(externalEditorLauncherProvider),
          initialLauncher,
        ),
        isTrue,
      );
      expect(
        identical(
          container.read(workspaceFileOpenCoordinatorProvider),
          initialCoordinator,
        ),
        isTrue,
      );

      await controller.updateEditor(
        (editor) => editor.copyWith(codeOpenTarget: CodeOpenTarget.zed),
      );
      final targetChangedCoordinator = container.read(
        workspaceFileOpenCoordinatorProvider,
      );

      expect(
        identical(
          container.read(externalEditorLauncherProvider),
          initialLauncher,
        ),
        isTrue,
      );
      expect(identical(targetChangedCoordinator, initialCoordinator), isFalse);

      await controller.updateEditor(
        (editor) => editor.copyWith(
          zedExecutableMode: ExternalEditorExecutableMode.custom,
          zedExecutablePath: r'C:\Tools\zed.exe',
        ),
      );

      expect(
        identical(
          container.read(externalEditorLauncherProvider),
          initialLauncher,
        ),
        isFalse,
      );
      expect(
        identical(
          container.read(workspaceFileOpenCoordinatorProvider),
          targetChangedCoordinator,
        ),
        isFalse,
      );
    },
  );
}
