import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/external_editor/presentation/external_editor_menu_entries.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('externalEditorMenuChildren', () {
    test('is empty when fewer than two editors are installed', () {
      final zed = externalEditorSpecs[ExternalEditorKind.zed]!;

      expect(
        externalEditorMenuChildren(resolved: zed, installed: const []),
        isEmpty,
      );
      expect(
        externalEditorMenuChildren(
          resolved: zed,
          installed: <ExternalEditorSpec>[zed],
        ),
        isEmpty,
      );
    });

    test('lists every installed editor and checks the resolved one', () {
      final zed = externalEditorSpecs[ExternalEditorKind.zed]!;
      final vscode = externalEditorSpecs[ExternalEditorKind.vscode]!;

      final children = externalEditorMenuChildren(
        resolved: zed,
        installed: <ExternalEditorSpec>[zed, vscode],
      ).whereType<AleraDropdownEntry<ExternalEditorKind>>().toList();

      expect(children.map((entry) => entry.value), <ExternalEditorKind>[
        ExternalEditorKind.zed,
        ExternalEditorKind.vscode,
      ]);
      expect(children.map((entry) => entry.label), <String>['Zed', 'VS Code']);
      expect(children.map((entry) => entry.selected), <bool>[true, false]);
    });
  });

  group('resolvedExternalEditorProvider', () {
    ProviderContainer container({
      required Set<ExternalEditorKind> installed,
      ExternalEditorKind configured = ExternalEditorKind.zed,
    }) {
      final container = ProviderContainer(
        overrides: [
          settingsControllerProvider.overrideWith(
            () => _FixedSettings(
              AleraSettings.defaults.copyWith(
                editor: AleraSettings.defaults.editor.copyWith(
                  externalEditor: configured,
                ),
              ),
            ),
          ),
          externalEditorLauncherForProvider.overrideWith(
            (ref, kind) => _StubLauncher(installed.contains(kind)),
          ),
        ],
      );
      addTearDown(container.dispose);
      return container;
    }

    test('is null when no editor resolves', () async {
      final c = container(installed: const <ExternalEditorKind>{});
      expect(await c.read(resolvedExternalEditorProvider.future), isNull);
    });

    test('prefers the configured editor when it is installed', () async {
      final c = container(
        installed: const <ExternalEditorKind>{
          ExternalEditorKind.zed,
          ExternalEditorKind.vscode,
        },
        configured: ExternalEditorKind.vscode,
      );
      final spec = await c.read(resolvedExternalEditorProvider.future);
      expect(spec?.kind, ExternalEditorKind.vscode);
    });

    test('falls back to the first installed editor', () async {
      final c = container(
        installed: const <ExternalEditorKind>{ExternalEditorKind.vscode},
      );
      final spec = await c.read(resolvedExternalEditorProvider.future);
      expect(spec?.kind, ExternalEditorKind.vscode);
    });
  });
}

class _StubLauncher implements ExternalEditorLauncher {
  const _StubLauncher(this.installed);

  final bool installed;

  @override
  Future<bool> isInstalled() async => installed;

  @override
  Future<ExternalEditorAvailability> checkAvailability() async =>
      ExternalEditorAvailability(available: installed);

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async => ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async => ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(
    String workspacePath,
  ) async => ExternalEditorLaunchResultFactories.opened;
}

class _FixedSettings extends SettingsController {
  _FixedSettings(this._settings);

  final AleraSettings _settings;

  @override
  AleraSettings build() => _settings;
}
