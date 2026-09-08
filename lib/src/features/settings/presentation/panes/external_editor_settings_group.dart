import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/design_system/forms/alera_setting_row.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/presentation/rows/settings_rows.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class const ExternalEditorSettingsGroup({
  super.key,
  required final EditorSettings settings,
  required final ValueChanged<EditorSettings Function(EditorSettings)>
  onChanged,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return AleraSettingsGroup(
      title: 'External Editor',
      description: 'Open workspaces and files in Zed without changing Alera\'s built-in editor behavior.',
      children: <Widget>[
        AleraSettingRow(
          title: 'Default Code Open Target',
          description: 'Choose where normal editable source and text files open. Dedicated Alera previews stay internal.',
          child: AleraDropdownField<CodeOpenTarget>(
            key: const ValueKey<String>('editor-default-code-open-target'),
            value: settings.codeOpenTarget,
            entries: const <AleraDropdownFieldEntry<CodeOpenTarget>>[
              AleraDropdownFieldEntry<CodeOpenTarget>(
                value: .alera,
                label: 'Alera',
              ),
              AleraDropdownFieldEntry<CodeOpenTarget>(
                value: .zed,
                label: 'Zed',
              ),
            ],
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(codeOpenTarget: value),
            ),
          ),
        ),
        SettingsSwitchRow(
          key: const ValueKey<String>('editor-zed-custom-executable-row'),
          title: 'Custom Zed Executable',
          description:
              'Off uses the zed command from the local command environment.',
          value: settings.zedExecutableMode == .custom,
          onChanged: (value) => onChanged(
            (settings) => settings.copyWith(
              zedExecutableMode: value ? .custom : .automatic,
            ),
          ),
        ),
        if (settings.zedExecutableMode == .custom)
          SettingsTextRow(
            key: const ValueKey<String>('editor-zed-executable-path-row'),
            title: 'Zed Executable',
            description: 'Full path to the Zed executable on this machine.',
            value: settings.zedExecutablePath ?? '',
            hintText: 'Path to zed or zed.exe',
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(
                zedExecutablePath: value.trim().isEmpty ? null : value.trim(),
              ),
            ),
          ),
        SettingsSwitchRow(
          key: const ValueKey<String>('editor-zed-new-window-row'),
          title: 'Open Workspaces in New Window',
          description: 'Use zed --new so each Alera worktree opens as a separate Zed workspace window.',
          value: settings.externalEditorWorkspaceMode == .newWindow,
          onChanged: (value) => onChanged(
            (settings) => settings.copyWith(
              externalEditorWorkspaceMode: value ? .newWindow : .defaultWindow,
            ),
          ),
        ),
        SettingsSwitchRow(
          key: const ValueKey<String>('editor-zed-auto-open-workspace-row'),
          title: 'Auto-open New Workspaces in Zed',
          description: 'After Alera creates a linked workspace, open that workspace in Zed automatically.',
          value: settings.autoOpenNewWorkspacesInZed,
          onChanged: (value) => onChanged(
            (settings) => settings.copyWith(autoOpenNewWorkspacesInZed: value),
          ),
        ),
        SettingsButtonRow(
          key: const ValueKey<String>('editor-zed-check-row'),
          title: 'Check Zed',
          description: 'Run a non-destructive zed --version check with the current executable setting.',
          buttonLabel: 'Check Zed',
          onPressed: () async {
            final availability = await ref
                .read(externalEditorLauncherProvider)
                .checkAvailability();
            if (!context.mounted) return;
            AleraToast.show(
              context,
              message: availability.available
                  ? 'Zed is available${availability.version == null ? '.' : ': ${availability.version}'}'
                  : availability.message ?? 'Zed is not available.',
              tone: availability.available
                  ? AleraToastTone.success
                  : AleraToastTone.error,
            );
          },
        ),
      ],
    );
  }
}
