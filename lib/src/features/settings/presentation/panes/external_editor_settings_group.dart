import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/design_system/forms/alera_setting_row.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
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
    final spec =
        externalEditorSpecs[settings.externalEditor] ??
        externalEditorSpecs.values.first;
    final customPath = settings.executablePathFor(spec.kind);
    return AleraSettingsGroup(
      title: 'External Editor',
      description: 'Open workspaces and files in an external editor without changing Alera\'s built-in editor behavior.',
      children: <Widget>[
        AleraSettingRow(
          title: 'External Editor',
          description: 'Editor used by Open In menu entries, keyboard shortcuts, and external file targets.',
          child: AleraDropdownField<ExternalEditorKind>(
            key: const ValueKey<String>('editor-external-editor-kind'),
            value: spec.kind,
            entries: <AleraDropdownFieldEntry<ExternalEditorKind>>[
              for (final spec in externalEditorSpecs.values)
                AleraDropdownFieldEntry<ExternalEditorKind>(
                  value: spec.kind,
                  label: spec.displayName,
                ),
            ],
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(externalEditor: value),
            ),
          ),
        ),
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
                value: .external,
                label: 'External Editor',
              ),
            ],
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(codeOpenTarget: value),
            ),
          ),
        ),
        SettingsSwitchRow(
          key: const ValueKey<String>('editor-external-custom-executable-row'),
          title: 'Custom ${spec.shortName} Executable',
          description:
              'Off uses the ${spec.commandCandidates.first} command from the local command environment.',
          value: customPath != null,
          onChanged: (value) => onChanged(
            (settings) => settings.copyWith(
              externalEditorExecutablePaths: _withExecutableOverride(
                settings,
                spec.kind,
                value ? (customPath ?? '') : null,
              ),
            ),
          ),
        ),
        if (customPath != null)
          SettingsTextRow(
            key: const ValueKey<String>('editor-external-executable-path-row'),
            title: '${spec.shortName} Executable',
            description:
                'Full path to the ${spec.displayName} executable on this machine.',
            value: customPath,
            hintText:
                'Path to ${spec.commandCandidates.first} or its executable',
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(
                externalEditorExecutablePaths: _withExecutableOverride(
                  settings,
                  spec.kind,
                  value.trim(),
                ),
              ),
            ),
          ),
        if (spec.supportsWorkspaceWindowMode)
          SettingsSwitchRow(
            key: const ValueKey<String>('editor-external-new-window-row'),
            title: 'Open Workspaces in New Window',
            description:
                'Open each Alera worktree as a separate ${spec.shortName} window instead of reusing the last one.',
            value: settings.externalEditorWorkspaceMode == .newWindow,
            onChanged: (value) => onChanged(
              (settings) => settings.copyWith(
                externalEditorWorkspaceMode: value
                    ? .newWindow
                    : .defaultWindow,
              ),
            ),
          ),
        SettingsSwitchRow(
          key: const ValueKey<String>(
            'editor-external-auto-open-workspace-row',
          ),
          title: 'Auto-open New Workspaces Externally',
          description:
              'After Alera creates a linked workspace, open that workspace in ${spec.displayName} automatically.',
          value: settings.autoOpenNewWorkspacesExternally,
          onChanged: (value) => onChanged(
            (settings) =>
                settings.copyWith(autoOpenNewWorkspacesExternally: value),
          ),
        ),
        SettingsButtonRow(
          key: const ValueKey<String>('editor-external-check-row'),
          title: 'Check ${spec.shortName}',
          description: 'Run a non-destructive version check with the current executable setting.',
          buttonLabel: 'Check ${spec.shortName}',
          onPressed: () async {
            final availability = await ref
                .read(externalEditorLauncherProvider)
                .checkAvailability();
            if (!context.mounted) return;
            AleraToast.show(
              context,
              message: availability.available
                  ? '${spec.displayName} is available${availability.version == null ? '.' : ': ${availability.version}'}'
                  : availability.message ??
                        '${spec.displayName} is not available.',
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

Map<String, String> _withExecutableOverride(
  EditorSettings settings,
  ExternalEditorKind kind,
  String? path,
) {
  final next = Map<String, String>.of(settings.externalEditorExecutablePaths);
  if (path == null) {
    next.remove(kind.name);
  } else {
    next[kind.name] = path;
  }
  return next;
}
