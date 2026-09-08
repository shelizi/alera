import 'package:alera/src/app/providers.dart';
import 'package:alera/src/features/workbench/application/terminal_composer_workspace_attachment.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator_provider.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_composer.dart';
import 'package:alera/src/features/workbench/presentation/terminal_composer_drop_target.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

TerminalComposerDropTarget buildTerminalComposerForWorkspace(
  WidgetRef ref,
  TerminalSessionHandle session,
) {
  return TerminalComposerDropTarget(
    key: session.composerController.dropTargetKey,
    session: session,
    child: TerminalComposer(
      session: session,
      onOpenWorkspaceFile: (filePath) =>
          openTerminalComposerWorkspaceFile(ref, session.workspaceId, filePath),
    ),
  );
}

Future<bool> openTerminalComposerWorkspaceFile(
  WidgetRef ref,
  String workspaceId,
  String filePath,
) async {
  final workspace = findWorkspaceById(
    ref.read(workbenchControllerProvider),
    workspaceId,
  );
  if (workspace == null) {
    return false;
  }
  return openTerminalComposerWorkspaceAttachment(
    workspacePath: workspace.path,
    filePath: filePath,
    workspaceFiles: ref.read(workspaceFileServiceProvider),
    openFile: (relativePath) async {
      await ref
          .read(workspaceFileOpenCoordinatorProvider)
          .open<WorkspaceTabRecord>(
            workspace: workspace,
            relativePath: relativePath,
            openInAlera:
                ({
                  required workspace,
                  required relativePath,
                  required preview,
                }) => ref
                    .read(workbenchControllerProvider.notifier)
                    .openFileTab(
                      workspace: workspace,
                      relativePath: relativePath,
                      preview: preview,
                    ),
          );
    },
  );
}
