import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/external_editor/infra/cli_external_editor_launcher.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'external_editor_providers.g.dart';

/// Launcher for the editor selected in Settings > Editor. Implicit flows
/// (code-open target, auto-open, keyboard shortcut fallback resolution) go
/// through this; explicit menu picks use [externalEditorLauncherForProvider].
@Riverpod(keepAlive: true)
ExternalEditorLauncher externalEditorLauncher(Ref ref) {
  final kind = ref.watch(
    settingsControllerProvider.select(
      (settings) => settings.editor.externalEditor,
    ),
  );
  return ref.watch(externalEditorLauncherForProvider(kind));
}

@Riverpod(keepAlive: true)
ExternalEditorLauncher externalEditorLauncherFor(
  Ref ref,
  ExternalEditorKind kind,
) {
  final editorConfig = ref.watch(
    settingsControllerProvider.select(
      (settings) => (
        executablePath: settings.editor.executablePathFor(kind),
        workspaceMode: settings.editor.externalEditorWorkspaceMode,
      ),
    ),
  );
  final spec = externalEditorSpecs[kind]!;
  return CliExternalEditorLauncher(
    spec: spec,
    processRunner: ref.watch(processRunnerProvider),
    commandReader: () => editorConfig.executablePath,
    workspaceModeReader: () => editorConfig.workspaceMode,
  );
}

/// Specs whose CLI resolves on this machine right now. Resolution-only; no
/// editor process is spawned, so menus can gate on it cheaply.
@Riverpod(keepAlive: true)
Future<List<ExternalEditorSpec>> installedExternalEditors(Ref ref) async {
  final results = await Future.wait(
    externalEditorSpecs.values.map(
      (spec) => ref
          .watch(externalEditorLauncherForProvider(spec.kind))
          .isInstalled()
          .then((installed) => installed ? spec : null),
    ),
  );
  return results.whereType<ExternalEditorSpec>().toList(growable: false);
}

/// The editor menus and shortcuts should act on: the configured kind when it
/// resolves, otherwise the first installed spec, otherwise null when nothing
/// is installed and every "Open in Editor" entry should hide.
@Riverpod(keepAlive: true)
Future<ExternalEditorSpec?> resolvedExternalEditor(Ref ref) async {
  final installed = await ref.watch(installedExternalEditorsProvider.future);
  if (installed.isEmpty) {
    return null;
  }
  final configured = ref.watch(
    settingsControllerProvider.select(
      (settings) => settings.editor.externalEditor,
    ),
  );
  for (final spec in installed) {
    if (spec.kind == configured) {
      return spec;
    }
  }
  return installed.first;
}
