part of 'workspace_git_history_ref_actions.dart';

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
      AleraToast.show(
        context,
        message: context.tr(errorMessage(error)),
        tone: .error,
      );
      return;
    }
    await _runGitHistoryMutation(
      context: context,
      operation: () =>
          backend.deleteBranch(repoPath: path, branch: branch, force: true),
      successMessage: context.trTemplate('Deleted branch {branch}', {
        'branch': branch,
      }),
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
    successMessage: context.trTemplate('Deleted branch {branch}', {
      'branch': branch,
    }),
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
        message: context.trTemplate('Could not parse remote branch {branch}', {
          'branch': remoteBranch,
        }),
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
    successMessage: context.trTemplate('Deleted {branch} on {remote}', {
      'branch': branch,
      'remote': remote,
    }),
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
      successMessage: context.trTemplate('Checked out {name}', {
        'name': localBranch,
      }),
      onMutationSuccess: onMutationSuccess,
    );
  } catch (error) {
    if (context.mounted) {
      AleraToast.show(
        context,
        message: context.tr(errorMessage(error)),
        tone: .error,
      );
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
    AleraToast.show(context, message: context.tr(message), tone: .error);
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
  AleraToast.show(context, message: context.tr(successMessage), tone: .success);
  await onMutationSuccess();
}

String _defaultGitHistoryArchiveName(String path, String ref) {
  final repoName = p.basename(p.normalize(path));
  final safeRef = ref.replaceAll(RegExp(r'[\\/:*?"<>|]+'), '-');
  return '${repoName.isEmpty ? 'repository' : repoName}-$safeRef.zip';
}
