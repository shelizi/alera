import 'package:alera/src/app/providers.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

Future<void> reloadWorkspaceEditorDocument(
  BuildContext context,
  WidgetRef ref,
  WorkspaceTabRecord tab,
) async {
  if (tab.kind != WorkspaceTabKind.editor) {
    return;
  }

  final registry = ref.read(editorSessionRegistryProvider);
  if (registry.isDirty(tab.id)) {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => const AleraConfirmDialog(
        title: 'Reload Document?',
        message:
            'Unsaved changes will be discarded before reloading from disk.',
        confirmLabel: 'Reload',
        destructive: true,
      ),
    );
    if (confirmed != true || !context.mounted) {
      return;
    }
    await registry.discard(tab.id);
    if (!context.mounted || registry.isDirty(tab.id)) {
      return;
    }
  }

  registry.reload(tab.id);
}
