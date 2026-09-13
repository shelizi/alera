import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/workbench/presentation/project_workbench_sidebar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

String? _menuEntryLabel(PopupMenuEntry<String> entry) {
  return switch (entry) {
    AleraDropdownEntry<String> e => e.label,
    AleraDropdownSubmenuEntry<String, dynamic> e => e.label,
    _ => null,
  };
}

List<String> _menuEntryLabels(List<PopupMenuEntry<String>> entries) {
  return entries.map(_menuEntryLabel).nonNulls.toList();
}

void main() {
  test(
    'section actions follow Parent and Clear Section requires membership',
    () {
      final labels = _menuEntryLabels(
        workspaceContextMenuEntries(
          fileManagerLabel: 'Files',
          hasClearParent: true,
          canRemove: true,
          isPinned: true,
          isArchived: false,
          supportsSections: true,
          hasSection: true,
          hasDescendants: true,
        ),
      );
      expect(
        labels.sublist(
          labels.indexOf('Pin Workspace Tree'),
          labels.indexOf('Clear Section') + 1,
        ),
        [
          'Pin Workspace Tree',
          'Unpin Workspace Tree',
          'Manage Tags',
          'Set Parent Workspace',
          'Clear Parent Workspace',
          'Set Section',
          'Clear Section',
        ],
      );
      final unassigned = _menuEntryLabels(
        workspaceContextMenuEntries(
          fileManagerLabel: 'Files',
          hasClearParent: false,
          canRemove: true,
          isPinned: false,
          isArchived: false,
          supportsSections: true,
        ),
      );
      expect(unassigned, contains('Set Section'));
      expect(unassigned, isNot(contains('Clear Section')));
      expect(unassigned, isNot(contains('Pin Workspace Tree')));
      expect(unassigned, isNot(contains('Unpin Workspace Tree')));
    },
  );

  test('workspace context menu places project settings with open actions', () {
    final entries = workspaceContextMenuEntries(
      fileManagerLabel: 'Files',
      hasClearParent: false,
      canRemove: true,
      isPinned: false,
      isArchived: false,
      externalEditor: externalEditorSpecs[ExternalEditorKind.zed],
    );

    expect(_menuEntryLabels(entries), <String>[
      'Rename',
      'Pin Workspace',
      'Manage Tags',
      'Set Parent Workspace',
      'Open in Browser',
      'Open in Files',
      'Open in Zed',
      'Open in Project Settings',
      'Copy Path',
      'Sleep',
      'Archive',
      'Remove',
    ]);
  });

  test('archived workspaces swap Sleep for Restore and hide Archive', () {
    final labels = _menuEntryLabels(
      workspaceContextMenuEntries(
        fileManagerLabel: 'Files',
        hasClearParent: false,
        canRemove: true,
        isPinned: false,
        isArchived: true,
      ),
    );

    expect(labels, contains('Restore'));
    expect(labels, isNot(contains('Sleep')));
    expect(labels, isNot(contains('Archive')));
  });

  test('the main workspace cannot archive since canRemove is false', () {
    final labels = _menuEntryLabels(
      workspaceContextMenuEntries(
        fileManagerLabel: 'Files',
        hasClearParent: false,
        canRemove: false,
        isPinned: false,
        isArchived: false,
      ),
    );

    expect(labels, contains('Sleep'));
    expect(labels, isNot(contains('Archive')));
  });
}
