export 'workspace_git_history_commit_menus.dart';
export 'workspace_git_history_input_dialogs.dart';

import 'workspace_git_history_commit_menus.dart';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';

import 'workspace_git_history_ref_menus.dart';

export 'workspace_git_history_ref_actions.dart';
export 'workspace_git_history_ref_menus.dart';

/// Shows the actions available for a history reference at a global pointer
/// position. Unsupported reference kinds do not open an empty menu.
Future<GitHistoryRefMenuAction?> showGitHistoryRefMenu(
  BuildContext context,
  GitHistoryItemRef itemRef,
  Offset globalPosition, {
  required bool isCurrentBranch,
  bool canPullIntoCurrentBranch = false,
}) async {
  final kind = _gitHistoryRefMenuKind(itemRef);
  return showGitHistoryRefMenuForKind(
    context,
    itemRef,
    globalPosition,
    kind: switch (kind) {
      _GitHistoryRefMenuKind.localBranch => .localBranch,
      _GitHistoryRefMenuKind.remoteBranch => .remoteBranch,
      _GitHistoryRefMenuKind.tag => .tag,
      _GitHistoryRefMenuKind.unsupported => .unsupported,
    },
    isCurrentBranch: isCurrentBranch,
    canPullIntoCurrentBranch: canPullIntoCurrentBranch,
  );
}

/// Shows the actions available for a non-boundary history item.
Future<GitHistoryCommitMenuAction?> showGitHistoryCommitMenu(
  BuildContext context,
  GitHistoryItem item,
  Offset globalPosition, {
  String? currentBranchName,
}) async {
  final position = _gitHistoryMenuPosition(context, globalPosition);
  if (position == null) {
    return null;
  }
  return showMenu<GitHistoryCommitMenuAction>(
    context: context,
    position: position,
    items: <PopupMenuEntry<GitHistoryCommitMenuAction>>[
      PopupMenuItem<GitHistoryCommitMenuAction>(
        key: const ValueKey<String>('git-history-commit-menu-identity'),
        enabled: false,
        height: 32,
        child: Text(
          '${context.tr('Commit')} · ${gitHistoryItemShortId(item)}',
          maxLines: 1,
          overflow: .ellipsis,
          style: Theme.of(context).textTheme.labelMedium?.copyWith(
            color: Theme.of(context).colorScheme.onSurface,
            fontWeight: .w700,
          ),
        ),
      ),
      const PopupMenuDivider(height: 1),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .copyHash,
        label: 'Copy Commit Hash',
        leading: Icon(AleraIcons.gitBranch, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .copySubject,
        label: 'Copy Commit Subject',
        leading: Icon(AleraIcons.copy, size: 16),
      ),
      const PopupMenuDivider(),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .addTag,
        label: 'Add Tag…',
        leading: Icon(AleraIcons.tag, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .createBranch,
        label: 'Create Branch Here…',
        leading: Icon(AleraIcons.gitBranch, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .openInWorktree,
        label: 'Create Worktree Here…',
        leading: Icon(AleraIcons.folderSpecial, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .checkoutCommit,
        label: 'Checkout Commit',
        leading: Icon(AleraIcons.forward, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .cherryPick,
        label: 'Cherry Pick',
        leading: Icon(AleraIcons.gitCommit, size: 16),
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .revertCommit,
        label: 'Revert Commit',
        leading: Icon(AleraIcons.restore, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .dropCommit,
        label: 'Drop Commit',
        enabled: item.parentIds.isNotEmpty,
        leading: const Icon(AleraIcons.delete, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .mergeIntoCurrentBranch,
        label: 'Merge Into Current Branch',
        enabled: currentBranchName?.trim().isNotEmpty ?? false,
        leading: const Icon(AleraIcons.gitMerge, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .rebaseCurrentBranch,
        label: 'Rebase Current Branch Onto This Commit',
        enabled: currentBranchName?.trim().isNotEmpty ?? false,
        leading: const Icon(AleraIcons.gitFork, size: 16),
      ),
      const PopupMenuDivider(),
      const AleraDropdownSubmenuEntry<
        GitHistoryCommitMenuAction,
        GitHistoryCommitMenuAction
      >(
        primaryValue: .resetMixed,
        label: 'Reset Current Branch Here',
        leading: Icon(AleraIcons.restart, size: 16),
        childResult: _resetModeResult,
        children: <PopupMenuEntry<GitHistoryCommitMenuAction>>[
          AleraDropdownEntry<GitHistoryCommitMenuAction>(
            value: .resetSoft,
            label: 'Soft',
          ),
          AleraDropdownEntry<GitHistoryCommitMenuAction>(
            value: .resetMixed,
            label: 'Mixed',
            selected: true,
          ),
          AleraDropdownEntry<GitHistoryCommitMenuAction>(
            value: .resetHard,
            label: 'Hard',
          ),
        ],
      ),
      const AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .createArchive,
        label: 'Create Archive…',
        leading: Icon(AleraIcons.archive, size: 16),
      ),
    ],
  );
}

GitHistoryCommitMenuAction _resetModeResult(GitHistoryCommitMenuAction value) =>
    value;

/// Asks the user to confirm detaching HEAD at [item]. The copy calls out the
/// detached state because commits made there do not move any branch.
Future<bool> showGitCheckoutCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Checkout Commit?',
      message:
          'Detaches HEAD at ${gitHistoryItemShortId(item)} "${item.subject}". '
          'Commits made in this state do not belong to a branch.',
      confirmLabel: 'Checkout',
    ),
  );
  return confirmed ?? false;
}

/// Asks the user to confirm reverting [item] on the current branch.
Future<bool> showGitRevertCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Revert Commit?',
      message:
          'Creates a new commit that reverts the changes in '
          '${gitHistoryItemShortId(item)} "${item.subject}".',
      confirmLabel: 'Revert',
    ),
  );
  return confirmed ?? false;
}

/// Asks the user to confirm moving the current branch to [item]. A hard reset
/// is marked destructive because it discards working tree changes.
Future<bool> showGitResetToCommitConfirmation(
  BuildContext context,
  GitHistoryItem item,
  GitResetMode mode,
) async {
  final shortId = gitHistoryItemShortId(item);
  final subject = item.subject;
  final message = switch (mode) {
    GitResetMode.soft =>
      'Moves the current branch to $shortId "$subject". Staged and working '
          'tree changes are kept.',
    GitResetMode.mixed =>
      'Moves the current branch to $shortId "$subject". Working tree '
          'changes are kept but become unstaged.',
    GitResetMode.hard =>
      'Moves the current branch to $shortId "$subject" and discards all '
          'working tree changes. This cannot be undone.',
  };
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Reset Current Branch?',
      message: message,
      confirmLabel: 'Reset',
      destructive: mode == GitResetMode.hard,
    ),
  );
  return confirmed ?? false;
}

RelativeRect? _gitHistoryMenuPosition(
  BuildContext context,
  Offset globalPosition,
) {
  final overlay = Navigator.of(context).overlay?.context.findRenderObject();
  if (overlay is! RenderBox) {
    return null;
  }
  final position = overlay.globalToLocal(globalPosition);
  return RelativeRect.fromLTRB(
    position.dx,
    position.dy,
    overlay.size.width - position.dx,
    overlay.size.height - position.dy,
  );
}

enum _GitHistoryRefMenuKind { localBranch, remoteBranch, tag, unsupported }

_GitHistoryRefMenuKind _gitHistoryRefMenuKind(GitHistoryItemRef itemRef) {
  final category = itemRef.category;
  if (category == GitHistoryRefCategory.branches ||
      (category == null && itemRef.id.startsWith('refs/heads/'))) {
    return _GitHistoryRefMenuKind.localBranch;
  }
  if (category == GitHistoryRefCategory.remoteBranches ||
      (category == null && itemRef.id.startsWith('refs/remotes/'))) {
    return _GitHistoryRefMenuKind.remoteBranch;
  }
  if (category == GitHistoryRefCategory.tags ||
      (category == null && itemRef.id.startsWith('refs/tags/'))) {
    return _GitHistoryRefMenuKind.tag;
  }
  return _GitHistoryRefMenuKind.unsupported;
}
