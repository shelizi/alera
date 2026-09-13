import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/alera_preview.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:flutter/material.dart';

@AleraPreview(name: 'Submenu', group: 'Dropdown Entry', size: Size(220, 140))
Widget aleraDropdownSubmenuEntryPreview() => Material(
  color: AleraTokens.surfaceElevated,
  borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
  child: Padding(
    padding: const EdgeInsets.all(AleraTokens.space8),
    child: Column(
      mainAxisSize: .min,
      children: <Widget>[
        AleraDropdownSubmenuEntry<String, String>(
          primaryValue: 'default',
          label: 'Open in Zed',
          childResult: (value) => value,
          leading: const Icon(AleraIcons.external, size: 16),
          children: <PopupMenuEntry<String>>[
            const AleraDropdownEntry<String>(
              value: 'zed',
              label: 'Zed',
              selected: true,
            ),
            const AleraDropdownEntry<String>(value: 'vscode', label: 'VS Code'),
          ],
        ),
        AleraDropdownSubmenuEntry<String, String>(
          primaryValue: null,
          label: 'Reset Current Branch Here',
          childResult: (value) => value,
          leading: const Icon(AleraIcons.restart, size: 16),
          children: const <PopupMenuEntry<String>>[
            AleraDropdownEntry<String>(value: 'soft', label: 'Soft'),
            AleraDropdownEntry<String>(value: 'mixed', label: 'Mixed'),
            AleraDropdownEntry<String>(value: 'hard', label: 'Hard'),
          ],
        ),
      ],
    ),
  ),
);
