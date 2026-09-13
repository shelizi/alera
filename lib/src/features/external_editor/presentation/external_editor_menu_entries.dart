import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:flutter/material.dart';

/// Children for an "Open in Editor ▸" submenu: one entry per installed editor
/// with the resolved one checked. Returns an empty list when fewer than two
/// editors resolve so the parent entry renders flat without the chevron.
List<PopupMenuEntry<ExternalEditorKind>> externalEditorMenuChildren({
  required ExternalEditorSpec resolved,
  required List<ExternalEditorSpec> installed,
}) {
  if (installed.length < 2) {
    return const <PopupMenuEntry<ExternalEditorKind>>[];
  }
  return <PopupMenuEntry<ExternalEditorKind>>[
    for (final spec in installed)
      AleraDropdownEntry<ExternalEditorKind>(
        value: spec.kind,
        label: spec.shortName,
        localizeLabel: false,
        selected: spec.kind == resolved.kind,
      ),
  ];
}
