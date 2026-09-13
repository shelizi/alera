import 'package:alera/src/features/ai_assist/application/ai_assist_errors.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_actions.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

import 'workspace_git_history_input_dialogs.dart';

import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;

enum GitHistoryCommitMenuAction {
  copyHash,
  copySubject,
  addTag,
  createBranch,
  openInWorktree,
  checkoutCommit,
  cherryPick,
  revertCommit,
  dropCommit,
  mergeIntoCurrentBranch,
  rebaseCurrentBranch,
  resetSoft,
  resetMixed,
  resetHard,
  createArchive,
}

/// Runs one of the commit actions that is shared by the graph and panel.
/// Returns true when [action] belongs to this shared delegate.
Future<bool> runGitHistoryCommitMenuAction({
  required BuildContext context,
  required GitHistoryCommitMenuAction action,
  required GitHistoryItem item,
  required GitBackend backend,
  required String path,
  required String? currentBranchName,
  required Future<void> Function() onMutationSuccess,
  required GitHistoryCreateWorktree onCreateWorktree,
}) async {
  switch (action) {
    case GitHistoryCommitMenuAction.addTag:
      final input = await showGitHistoryTagInputDialog(context);
      if (input == null || !context.mounted) {
        return true;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.createTag(
          path: path,
          commitId: item.id,
          name: input.name,
          message: input.message,
        ),
        successMessage: () => 'Added tag ${input.name}',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.createBranch:
      final branch = await showGitHistoryBranchInputDialog(context);
      if (branch == null || !context.mounted) {
        return true;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.createBranchAtCommit(
          path: path,
          commitId: item.id,
          branch: branch,
        ),
        successMessage: () =>
            'Created branch $branch at ${gitHistoryItemShortId(item)}',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.openInWorktree:
      await runGitHistoryOpenInWorktree(
        context: context,
        backend: backend,
        path: path,
        sourceRef: item.id,
        promptForBranch: true,
        createBranchAtRef: true,
        onCreateWorktree: onCreateWorktree,
        onMutationSuccess: onMutationSuccess,
        errorMessage: gitHistoryErrorMessage,
      );
    case GitHistoryCommitMenuAction.cherryPick:
      await _runGitHistoryMutation(
        context: context,
        operation: () async {
          await backend.cherryPickCommit(
            path: path,
            commitId: item.id,
            mainlineParent: item.parentIds.length > 1 ? 1 : null,
          );
        },
        successMessage: () => 'Cherry-picked ${gitHistoryItemShortId(item)}',
        conflictMessage: 'Cherry pick was aborted because of conflicts.',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.dropCommit:
      if (item.parentIds.isEmpty) {
        return true;
      }
      final confirmed = await showGitDropCommitConfirmation(context, item);
      if (!confirmed || !context.mounted) {
        return true;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.dropCommit(path: path, commitId: item.id),
        successMessage: () => 'Dropped ${gitHistoryItemShortId(item)}',
        conflictMessage: 'Drop was aborted because of conflicts.',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.mergeIntoCurrentBranch:
      final branch = currentBranchName?.trim();
      if (branch == null || branch.isEmpty) {
        return true;
      }
      final confirmed = await showGitMergeCommitConfirmation(
        context,
        item,
        branch,
      );
      if (!confirmed || !context.mounted) {
        return true;
      }
      String? mergeCommitId;
      await _runGitHistoryMutation(
        context: context,
        operation: () async {
          mergeCommitId = await backend.mergeRef(path: path, ref: item.id);
        },
        successMessage: () => mergeCommitId == null
            ? 'Current branch fast-forwarded to ${gitHistoryItemShortId(item)}'
            : 'Merge commit created from ${gitHistoryItemShortId(item)}',
        conflictMessage: 'Merge stopped with conflicts. Resolve conflicts in Source Control.',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.rebaseCurrentBranch:
      final branch = currentBranchName?.trim();
      if (branch == null || branch.isEmpty) {
        return true;
      }
      final confirmed = await showGitRebaseCommitConfirmation(
        context,
        item,
        branch,
      );
      if (!confirmed || !context.mounted) {
        return true;
      }
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.rebaseOnto(path: path, ontoRef: item.id),
        successMessage: () =>
            'Rebased $branch onto ${gitHistoryItemShortId(item)}',
        conflictMessage: 'Rebase was aborted because of conflicts.',
        onMutationSuccess: onMutationSuccess,
      );
    case GitHistoryCommitMenuAction.createArchive:
      final repositoryName = p.basename(p.normalize(path));
      final initialPath = p.join(
        path,
        '$repositoryName-${gitHistoryItemShortId(item)}.zip',
      );
      final outputPath = await showGitHistoryArchiveInputDialog(
        context,
        initialPath: initialPath,
      );
      if (outputPath == null || !context.mounted) {
        return true;
      }
      final format = p.extension(outputPath).toLowerCase() == '.tar'
          ? GitArchiveFormat.tar
          : GitArchiveFormat.zip;
      await _runGitHistoryMutation(
        context: context,
        operation: () => backend.createArchive(
          path: path,
          ref: item.id,
          outputPath: outputPath,
          format: format,
        ),
        successMessage: () => 'Created archive at $outputPath',
        onMutationSuccess: onMutationSuccess,
        refreshAfterSuccess: false,
      );
    case GitHistoryCommitMenuAction.copyHash:
    case GitHistoryCommitMenuAction.copySubject:
    case GitHistoryCommitMenuAction.checkoutCommit:
    case GitHistoryCommitMenuAction.revertCommit:
    case GitHistoryCommitMenuAction.resetSoft:
    case GitHistoryCommitMenuAction.resetMixed:
    case GitHistoryCommitMenuAction.resetHard:
      return false;
  }
  return true;
}

Future<void> _runGitHistoryMutation({
  required BuildContext context,
  required Future<void> Function() operation,
  required String Function() successMessage,
  required Future<void> Function() onMutationSuccess,
  String? conflictMessage,
  bool refreshAfterSuccess = true,
}) async {
  try {
    await operation();
    if (!context.mounted) {
      return;
    }
    AleraToast.show(context, message: successMessage(), tone: .success);
    if (refreshAfterSuccess) {
      await onMutationSuccess();
    }
  } catch (error) {
    if (!context.mounted) {
      return;
    }
    AleraToast.show(
      context,
      message: conflictMessage ?? gitHistoryErrorMessage(error),
      tone: .error,
    );
  }
}

/// Shares the Git exception wording used by history actions with the source
/// control panel's existing error mapping.
String gitHistoryErrorMessage(Object error) {
  if (error is NotARepositoryException) {
    return 'This workspace is not a Git repository.';
  }
  if (error is DetachedHeadException) {
    return 'Cannot run this action from detached HEAD.';
  }
  if (error is RemoteNotFoundException) {
    return 'Remote origin was not found.';
  }
  if (error is NothingToCommitException) {
    return 'Nothing to commit.';
  }
  if (error is GitConflictException) {
    return 'Resolve conflicts before continuing.';
  }
  if (error is WorkspaceException) {
    return error.toString();
  }
  if (error is AiAssistException) {
    return error.message;
  }
  if (error is GitException && error.context.trim().isNotEmpty) {
    return error.context;
  }
  return 'Git operation failed.';
}

/// Short commit id used in confirmation copy and toasts.
String gitHistoryItemShortId(GitHistoryItem item) =>
    item.id.length > 7 ? item.id.substring(0, 7) : item.id;

/// Asks the user to confirm dropping [item] from the current branch.
Future<bool> showGitDropCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Drop Commit?',
      message:
          'Removes ${gitHistoryItemShortId(item)} "${item.subject}" from '
          'the current branch. This cannot be undone.',
      confirmLabel: 'Drop',
      destructive: true,
    ),
  );
  return confirmed ?? false;
}

/// Asks the user to confirm merging [item] into [currentBranchName].
Future<bool> showGitMergeCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
  String currentBranchName,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Merge Commit?',
      message: 'Merge ${gitHistoryItemShortId(item)} into $currentBranchName?',
      confirmLabel: 'Merge',
    ),
  );
  return confirmed ?? false;
}

/// Asks the user to confirm rebasing [currentBranchName] onto [item].
Future<bool> showGitRebaseCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
  String currentBranchName,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Rebase Current Branch?',
      message:
          'Rebase $currentBranchName onto ${gitHistoryItemShortId(item)} '
          '"${item.subject}". This rewrites the current branch history.',
      confirmLabel: 'Rebase',
      destructive: true,
    ),
  );
  return confirmed ?? false;
}
