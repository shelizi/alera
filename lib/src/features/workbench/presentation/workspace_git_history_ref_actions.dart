import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_input_dialogs.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_dialogs.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_menus.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;

typedef GitHistoryRefMenuCallback = Future<void> Function(
  GitHistoryItemRef itemRef,
  GitHistoryRefMenuAction action, {
  required bool isCurrentBranch,
  required bool isCurrentUpstream,
  required String? currentBranch,
});

typedef GitHistoryBoundaryMenuCallback = Future<void> Function(
  GitHistoryBoundaryMenuAction action,
);

/// Host-side linked-worktree creation. The arguments map onto
/// `WorkbenchController.createWorkspace`; [sourceBranch] is ignored when
/// [reuseExistingBranch] is set.
typedef GitHistoryCreateWorktree = Future<void> Function({
  required String sourceBranch,
  required String newBranchName,
  required bool reuseExistingBranch,
});

Future<void> runGitHistoryRefMenuAction({
  required BuildContext context,
  required GitBackend backend,
  required String path,
  required GitHistoryItemRef itemRef,
  required GitHistoryRefMenuAction action,
  required bool isCurrentBranch,
  required bool isCurrentUpstream,
  required String? currentBranch,
  required Future<void> Function(String branch)? onSwitchBranch,
  required Future<void> Function(String text, String label) onCopyName,
  required Future<void> Function() onPull,
  required Future<void> Function() onMutationSuccess,
  required GitHistoryCreateWorktree onCreateWorktree,
  required String Function(Object error) errorMessage,
}) async {
  switch (action) {
    case GitHistoryRefMenuAction.switchBranch:
      if (onSwitchBranch != null) {
        await onSwitchBranch(itemRef.name);
      }
    case GitHistoryRefMenuAction.copyName:
      await onCopyName(
        itemRef.name,
        classifyGitHistoryRef(itemRef) == GitHistoryRefMenuKind.tag
            ? 'Tag Name'
            : 'Branch Name',
      );
    case GitHistoryRefMenuAction.renameBranch:
      final newName = await showGitHistoryRenameBranchDialog(
        context,
        itemRef.name,
      );
      if (newName == null || !context.mounted) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.renameBranch(
          path: path,
          oldName: itemRef.name,
          newName: newName,
        ),
        successMessage: 'Renamed ${itemRef.name} to $newName',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.deleteBranch:
      if (isCurrentBranch) {
        return;
      }
      await _deleteBranch(
        context: context,
        backend: backend,
        path: path,
        branch: itemRef.name,
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.mergeIntoCurrentBranch:
      final confirmed = await showGitHistoryMergeConfirmation(
        context,
        branch: itemRef.name,
        currentBranch: currentBranch ?? 'Current Branch',
      );
      if (!confirmed || !context.mounted) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () async {
          await backend.mergeRef(path: path, ref: itemRef.name);
        },
        successMessage: 'Merged ${itemRef.name}',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
        conflictMessage:
            'Merge conflicts occurred. Resolve conflicts in Source Control.',
      );
    case GitHistoryRefMenuAction.rebaseOntoCurrentBranch:
      final confirmed = await showGitHistoryRebaseConfirmation(
        context,
        itemRef.name,
      );
      if (!confirmed || !context.mounted) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.rebaseOnto(path: path, ontoRef: itemRef.name),
        successMessage: 'Rebased onto ${itemRef.name}',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
        conflictMessage: 'Rebase conflicts were aborted. Resolve conflicts in Source Control.',
      );
    case GitHistoryRefMenuAction.createArchive:
      final outputPath = await showGitHistoryArchivePathDialog(
        context,
        initialValue: _defaultGitHistoryArchiveName(path, itemRef.name),
      );
      if (outputPath == null || !context.mounted) {
        return;
      }
      final format = p.extension(outputPath).toLowerCase() == '.tar'
          ? GitArchiveFormat.tar
          : GitArchiveFormat.zip;
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.createArchive(
          path: path,
          ref: itemRef.name,
          outputPath: outputPath,
          format: format,
        ),
        successMessage: 'Created archive $outputPath',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.checkoutRemoteBranch:
      await _checkoutRemoteBranch(
        context: context,
        backend: backend,
        path: path,
        remoteBranch: itemRef.name,
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.deleteRemoteBranch:
      await _deleteRemoteBranch(
        context: context,
        backend: backend,
        path: path,
        remoteBranch: itemRef.name,
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.openInWorktree:
      final kind = classifyGitHistoryRef(itemRef);
      await runGitHistoryOpenInWorktree(
        context: context,
        backend: backend,
        path: path,
        sourceRef: itemRef.name,
        promptForBranch: kind != GitHistoryRefMenuKind.localBranch,
        createBranchAtRef: kind == GitHistoryRefMenuKind.tag,
        initialBranchName: kind == GitHistoryRefMenuKind.remoteBranch
            ? itemRef.name.substring(itemRef.name.indexOf('/') + 1)
            : itemRef.name,
        onCreateWorktree: onCreateWorktree,
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.pullIntoCurrentBranch:
      if (!isCurrentUpstream) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: onPull,
        successMessage: 'Pulled ${itemRef.name}',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.pushTag:
      await _runGitHistoryMutation(
        context: context,
        operation: () =>
            backend.pushTag(path: path, name: itemRef.name, remote: 'origin'),
        successMessage: 'Pushed tag ${itemRef.name}',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryRefMenuAction.deleteTag:
      final confirmed = await showGitHistoryDeleteTagConfirmation(
        context,
        itemRef.name,
      );
      if (!confirmed || !context.mounted) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.deleteTag(path: path, name: itemRef.name),
        successMessage: 'Deleted tag ${itemRef.name}',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
  }
}

Future<void> runGitHistoryBoundaryMenuAction({
  required BuildContext context,
  required GitHistoryBoundaryMenuAction action,
  required Future<void> Function() onStash,
  required Future<void> Function() onDiscardAll,
  required VoidCallback onCommitChanges,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
}) async {
  switch (action) {
    case GitHistoryBoundaryMenuAction.stashChanges:
      await _runGitHistoryMutation(
        context: context,
        operation: onStash,
        successMessage: 'Stashed changes',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryBoundaryMenuAction.discardAllChanges:
      final confirmed = await showGitHistoryDiscardAllChangesConfirmation(
        context,
      );
      if (!confirmed || !context.mounted) {
        return;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: onDiscardAll,
        successMessage: 'Discarded all changes',
        onMutationSuccess: onMutationSuccess,
        errorMessage: errorMessage,
      );
    case GitHistoryBoundaryMenuAction.commitChanges:
      onCommitChanges();
  }
}

/// Opens a linked worktree for [sourceRef]. Local branches reuse the ref
/// directly; remote-tracking refs let `createWorkspace` create the tracking
/// branch (preserving upstream); commits and tags get a local branch created
/// at [sourceRef] first because the worktree checkout needs a branch.
Future<void> runGitHistoryOpenInWorktree({
  required BuildContext context,
  required GitBackend backend,
  required String path,
  required String sourceRef,
  required bool promptForBranch,
  required bool createBranchAtRef,
  String? initialBranchName,
  required GitHistoryCreateWorktree onCreateWorktree,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
}) async {
  var branch = sourceRef;
  if (promptForBranch) {
    final input = await showGitHistoryWorktreeInputDialog(
      context,
      initialValue: initialBranchName ?? '',
    );
    if (input == null || !context.mounted) {
      return;
    }
    branch = input;
  }
  try {
    if (createBranchAtRef) {
      await backend.createBranchAtCommit(
        path: path,
        commitId: sourceRef,
        branch: branch,
      );
      if (!context.mounted) {
        return;
      }
    }
    await onCreateWorktree(
      sourceBranch: createBranchAtRef ? branch : sourceRef,
      newBranchName: branch,
      reuseExistingBranch: !promptForBranch || createBranchAtRef,
    );
    await onMutationSuccess();
  } catch (error) {
    if (context.mounted) {
      AleraToast.show(context, message: errorMessage(error), tone: .error);
    }
  }
}

Future<void> _deleteBranch({
  required BuildContext context,
  required GitBackend backend,
  required String path,
  required String branch,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
}) async {
  final confirmed = await showGitHistoryDeleteBranchConfirmation(
    context,
    branch,
  );
  if (!confirmed || !context.mounted) {
    return;
  }
  try {
    await backend.deleteBranch(repoPath: path, branch: branch, force: false);
  } catch (error) {
    if (!context.mounted) {
      return;
    }
    final force = await showGitHistoryForceDeleteBranchConfirmation(
      context,
      branch,
    );
    if (!context.mounted) {
      return;
    }
    if (!force) {
      AleraToast.show(context, message: errorMessage(error), tone: .error);
      return;
    }
    await _runGitHistoryMutation(
      context: context,
      operation: () =>
          backend.deleteBranch(repoPath: path, branch: branch, force: true),
      successMessage: 'Deleted branch $branch',
      onMutationSuccess: onMutationSuccess,
      errorMessage: errorMessage,
    );
    return;
  }
  if (!context.mounted) {
    return;
  }
  await _completeGitHistoryMutation(
    context: context,
    successMessage: 'Deleted branch $branch',
    onMutationSuccess: onMutationSuccess,
  );
}

Future<void> _deleteRemoteBranch({
  required BuildContext context,
  required GitBackend backend,
  required String path,
  required String remoteBranch,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
}) async {
  final separator = remoteBranch.indexOf('/');
  if (separator <= 0 || separator == remoteBranch.length - 1) {
    if (context.mounted) {
      AleraToast.show(
        context,
        message: 'Could not parse remote branch $remoteBranch',
        tone: .error,
      );
    }
    return;
  }
  final remote = remoteBranch.substring(0, separator);
  final branch = remoteBranch.substring(separator + 1);
  final confirmed = await showGitHistoryDeleteRemoteBranchConfirmation(
    context,
    remote: remote,
    branch: branch,
  );
  if (!confirmed || !context.mounted) {
    return;
  }
  await _runGitHistoryMutation(
    context: context,
    operation: () =>
        backend.deleteRemoteBranch(path: path, remote: remote, branch: branch),
    successMessage: 'Deleted $branch on $remote',
    onMutationSuccess: onMutationSuccess,
    errorMessage: errorMessage,
  );
}

Future<void> _checkoutRemoteBranch({
  required BuildContext context,
  required GitBackend backend,
  required String path,
  required String remoteBranch,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
}) async {
  try {
    final localBranch = await backend.checkoutRemoteBranch(
      path: path,
      remoteBranch: remoteBranch,
    );
    if (!context.mounted) {
      return;
    }
    await _completeGitHistoryMutation(
      context: context,
      successMessage: 'Checked out $localBranch',
      onMutationSuccess: onMutationSuccess,
    );
  } catch (error) {
    if (context.mounted) {
      AleraToast.show(context, message: errorMessage(error), tone: .error);
    }
  }
}

Future<void> _runGitHistoryMutation({
  required BuildContext context,
  required Future<void> Function() operation,
  required String successMessage,
  required Future<void> Function() onMutationSuccess,
  required String Function(Object error) errorMessage,
  String? conflictMessage,
}) async {
  try {
    await operation();
    if (!context.mounted) {
      return;
    }
    await _completeGitHistoryMutation(
      context: context,
      successMessage: successMessage,
      onMutationSuccess: onMutationSuccess,
    );
  } catch (error) {
    if (!context.mounted) {
      return;
    }
    final message = error is GitConflictException && conflictMessage != null
        ? conflictMessage
        : errorMessage(error);
    AleraToast.show(context, message: message, tone: .error);
  }
}

Future<void> _completeGitHistoryMutation({
  required BuildContext context,
  required String successMessage,
  required Future<void> Function() onMutationSuccess,
}) async {
  if (!context.mounted) {
    return;
  }
  AleraToast.show(context, message: successMessage, tone: .success);
  await onMutationSuccess();
}

String _defaultGitHistoryArchiveName(String path, String ref) {
  final repoName = p.basename(p.normalize(path));
  final safeRef = ref.replaceAll(RegExp(r'[\\/:*?"<>|]+'), '-');
  return '${repoName.isEmpty ? 'repository' : repoName}-$safeRef.zip';
}
