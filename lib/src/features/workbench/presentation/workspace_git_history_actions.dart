import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';

import 'workspace_git_history_ref_menus.dart';

export 'workspace_git_history_ref_actions.dart';
export 'workspace_git_history_ref_menus.dart';

enum GitHistoryCommitMenuAction {
  copyHash,
  copySubject,
  checkoutCommit,
  revertCommit,
  resetSoft,
  resetMixed,
  resetHard,
}

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

/// Shows the copy actions available for a non-boundary history item.
Future<GitHistoryCommitMenuAction?> showGitHistoryCommitMenu(
  BuildContext context,
  GitHistoryItem item,
  Offset globalPosition,
) async {
  final position = _gitHistoryMenuPosition(context, globalPosition);
  if (position == null) {
    return null;
  }
  return showMenu<GitHistoryCommitMenuAction>(
    context: context,
    position: position,
    items: const <PopupMenuEntry<GitHistoryCommitMenuAction>>[
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .copyHash,
        label: 'Copy Commit Hash',
        localizeLabel: false,
        leading: Icon(AleraIcons.gitBranch, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .copySubject,
        label: 'Copy Commit Subject',
        localizeLabel: false,
        leading: Icon(AleraIcons.copy, size: 16),
      ),
      PopupMenuDivider(),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .checkoutCommit,
        label: 'Checkout Commit',
        localizeLabel: false,
        leading: Icon(AleraIcons.forward, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .revertCommit,
        label: 'Revert Commit',
        localizeLabel: false,
        leading: Icon(AleraIcons.restore, size: 16),
      ),
      PopupMenuDivider(),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .resetSoft,
        label: 'Reset Current Branch Here (Soft)',
        localizeLabel: false,
        leading: Icon(AleraIcons.restart, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .resetMixed,
        label: 'Reset Current Branch Here (Mixed)',
        localizeLabel: false,
        leading: Icon(AleraIcons.restart, size: 16),
      ),
      AleraDropdownEntry<GitHistoryCommitMenuAction>(
        value: .resetHard,
        label: 'Reset Current Branch Here (Hard)',
        localizeLabel: false,
        leading: Icon(AleraIcons.restart, size: 16),
      ),
    ],
  );
}

/// Short commit id used in confirmation copy and toasts.
String gitHistoryItemShortId(GitHistoryItem item) =>
    item.id.length > 7 ? item.id.substring(0, 7) : item.id;

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
