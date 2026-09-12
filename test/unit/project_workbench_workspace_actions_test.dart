import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/features/workbench/presentation/project_workbench_sidebar.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'section actions follow Parent and Clear Section requires membership',
    () {
      final labels =
          workspaceContextMenuEntries(
                fileManagerLabel: 'Files',
                hasClearParent: true,
                canRemove: true,
                isPinned: true,
                isArchived: false,
                supportsSections: true,
                hasSection: true,
                hasDescendants: true,
              )
              .whereType<AleraDropdownEntry<String>>()
              .map((entry) => entry.label)
              .toList();
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
      final unassigned = workspaceContextMenuEntries(
        fileManagerLabel: 'Files',
        hasClearParent: false,
        canRemove: true,
        isPinned: false,
        isArchived: false,
        supportsSections: true,
      ).whereType<AleraDropdownEntry<String>>().map((entry) => entry.label);
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
    );

    expect(
      entries.whereType<AleraDropdownEntry<String>>().map(
        (entry) => entry.label,
      ),
      <String>[
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
      ],
    );
  });

  test('archived workspaces swap Sleep for Restore and hide Archive', () {
    final labels = workspaceContextMenuEntries(
      fileManagerLabel: 'Files',
      hasClearParent: false,
      canRemove: true,
      isPinned: false,
      isArchived: true,
    ).whereType<AleraDropdownEntry<String>>().map((entry) => entry.label);

    expect(labels, contains('Restore'));
    expect(labels, isNot(contains('Sleep')));
    expect(labels, isNot(contains('Archive')));
  });

  test('the main workspace cannot archive since canRemove is false', () {
    final labels = workspaceContextMenuEntries(
      fileManagerLabel: 'Files',
      hasClearParent: false,
      canRemove: false,
      isPinned: false,
      isArchived: false,
    ).whereType<AleraDropdownEntry<String>>().map((entry) => entry.label);

    expect(labels, contains('Sleep'));
    expect(labels, isNot(contains('Archive')));
  });
}
