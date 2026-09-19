import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';

enum GitHistoryRefMenuAction {
  switchBranch,
  renameBranch,
  deleteBranch,
  mergeIntoCurrentBranch,
  rebaseOntoCurrentBranch,
  createArchive,
  checkoutRemoteBranch,
  deleteRemoteBranch,
  pullIntoCurrentBranch,
  pushTag,
  deleteTag,
  openInWorktree,
  copyName,
}

enum GitHistoryBoundaryMenuAction {
  stashChanges,
  discardAllChanges,
  commitChanges,
}

enum GitHistoryRefMenuKind { localBranch, remoteBranch, tag, unsupported }

GitHistoryRefMenuKind classifyGitHistoryRef(GitHistoryItemRef itemRef) {
  final category = itemRef.category;
  if (category == GitHistoryRefCategory.branches ||
      (category == null && itemRef.id.startsWith('refs/heads/'))) {
    return GitHistoryRefMenuKind.localBranch;
  }
  if (category == GitHistoryRefCategory.remoteBranches ||
      (category == null && itemRef.id.startsWith('refs/remotes/'))) {
    return GitHistoryRefMenuKind.remoteBranch;
  }
  if (category == GitHistoryRefCategory.tags ||
      (category == null && itemRef.id.startsWith('refs/tags/'))) {
    return GitHistoryRefMenuKind.tag;
  }
  return GitHistoryRefMenuKind.unsupported;
}

bool gitHistoryRefMatchesUpstream(
  GitHistoryItemRef itemRef, {
  GitHistoryItemRef? remoteRef,
  String? upstream,
}) {
  if (remoteRef != null &&
      (itemRef.id == remoteRef.id || itemRef.name == remoteRef.name)) {
    return true;
  }
  if (upstream == null || upstream.isEmpty) {
    return false;
  }
  return itemRef.name == upstream || itemRef.id == 'refs/remotes/$upstream';
}

Future<GitHistoryRefMenuAction?> showGitHistoryRefMenuForKind(
  BuildContext context,
  GitHistoryItemRef itemRef,
  Offset globalPosition, {
  required GitHistoryRefMenuKind kind,
  required bool isCurrentBranch,
  bool canPullIntoCurrentBranch = false,
}) async {
  if (kind == GitHistoryRefMenuKind.unsupported) {
    return null;
  }
  final position = _gitHistoryRefMenuPosition(context, globalPosition);
  if (position == null) {
    return null;
  }
  final copyLabel = kind == GitHistoryRefMenuKind.tag
      ? 'Copy Tag Name'
      : 'Copy Branch Name';
  final identityLabel = switch (kind) {
    GitHistoryRefMenuKind.tag => 'Tag · ${itemRef.name}',
    GitHistoryRefMenuKind.localBranch ||
    GitHistoryRefMenuKind.remoteBranch => 'Branch · ${itemRef.name}',
    GitHistoryRefMenuKind.unsupported => itemRef.name,
  };
  return showMenu<GitHistoryRefMenuAction>(
    context: context,
    position: position,
    items: <PopupMenuEntry<GitHistoryRefMenuAction>>[
      PopupMenuItem<GitHistoryRefMenuAction>(
        key: const ValueKey<String>('git-history-ref-menu-identity'),
        enabled: false,
        height: 32,
        child: Text(
          identityLabel,
          maxLines: 1,
          overflow: .ellipsis,
          style: Theme.of(context).textTheme.labelMedium?.copyWith(
            color: Theme.of(context).colorScheme.onSurface,
            fontWeight: .w700,
          ),
        ),
      ),
      const PopupMenuDivider(height: 1),
      if (kind == GitHistoryRefMenuKind.localBranch) ...<
        PopupMenuEntry<GitHistoryRefMenuAction>
      >[
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .switchBranch,
          label: isCurrentBranch ? 'Current Branch' : 'Switch to Branch',
          localizeLabel: false,
          selected: isCurrentBranch,
          enabled: !isCurrentBranch,
          leading: const Icon(AleraIcons.gitBranch, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .renameBranch,
          label: 'Rename Branch...',
          localizeLabel: false,
          leading: Icon(AleraIcons.edit, size: 16),
        ),
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .deleteBranch,
          label: 'Delete Branch',
          localizeLabel: false,
          enabled: !isCurrentBranch,
          leading: const Icon(AleraIcons.delete, size: 16),
        ),
        const PopupMenuDivider(),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .mergeIntoCurrentBranch,
          label: 'Merge into Current Branch',
          localizeLabel: false,
          leading: Icon(AleraIcons.gitMerge, size: 16),
        ),
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .rebaseOntoCurrentBranch,
          label: 'Rebase Current Branch onto ${itemRef.name}',
          localizeLabel: false,
          leading: const Icon(AleraIcons.gitFork, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .createArchive,
          label: 'Create Archive...',
          localizeLabel: false,
          leading: Icon(AleraIcons.archive, size: 16),
        ),
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .openInWorktree,
          label: 'Open in New Worktree',
          localizeLabel: false,
          enabled: !isCurrentBranch,
          leading: const Icon(AleraIcons.folderSpecial, size: 16),
        ),
        const PopupMenuDivider(),
      ] else if (kind == GitHistoryRefMenuKind.remoteBranch) ...<
        PopupMenuEntry<GitHistoryRefMenuAction>
      >[
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .checkoutRemoteBranch,
          label: 'Checkout Remote Branch',
          localizeLabel: false,
          leading: Icon(AleraIcons.gitBranch, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .deleteRemoteBranch,
          label: 'Delete Remote Branch',
          localizeLabel: false,
          leading: Icon(AleraIcons.delete, size: 16),
        ),
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .pullIntoCurrentBranch,
          label: 'Pull into Current Branch',
          localizeLabel: false,
          enabled: canPullIntoCurrentBranch,
          leading: const Icon(AleraIcons.gitPull, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .openInWorktree,
          label: 'Open in New Worktree...',
          localizeLabel: false,
          leading: Icon(AleraIcons.folderSpecial, size: 16),
        ),
        const PopupMenuDivider(),
      ] else ...<PopupMenuEntry<GitHistoryRefMenuAction>>[
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .pushTag,
          label: 'Push Tag',
          localizeLabel: false,
          leading: Icon(AleraIcons.gitPush, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .deleteTag,
          label: 'Delete Tag',
          localizeLabel: false,
          leading: Icon(AleraIcons.delete, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .createArchive,
          label: 'Create Archive...',
          localizeLabel: false,
          leading: Icon(AleraIcons.archive, size: 16),
        ),
        const AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .openInWorktree,
          label: 'Open in New Worktree...',
          localizeLabel: false,
          leading: Icon(AleraIcons.folderSpecial, size: 16),
        ),
        const PopupMenuDivider(),
      ],
      AleraDropdownEntry<GitHistoryRefMenuAction>(
        value: .copyName,
        label: copyLabel,
        localizeLabel: false,
        leading: const Icon(AleraIcons.copy, size: 16),
      ),
    ],
  );
}

Future<GitHistoryBoundaryMenuAction?> showGitHistoryBoundaryMenu(
  BuildContext context,
  Offset globalPosition,
) async {
  final position = _gitHistoryRefMenuPosition(context, globalPosition);
  if (position == null) {
    return null;
  }
  return showMenu<GitHistoryBoundaryMenuAction>(
    context: context,
    position: position,
    items: const <PopupMenuEntry<GitHistoryBoundaryMenuAction>>[
      AleraDropdownEntry<GitHistoryBoundaryMenuAction>(
        value: .stashChanges,
        label: 'Stash Changes',
        localizeLabel: false,
        leading: Icon(AleraIcons.gitStash, size: 16),
      ),
      AleraDropdownEntry<GitHistoryBoundaryMenuAction>(
        value: .discardAllChanges,
        label: 'Discard All Changes',
        localizeLabel: false,
        leading: Icon(AleraIcons.gitDiscard, size: 16),
      ),
      PopupMenuDivider(),
      AleraDropdownEntry<GitHistoryBoundaryMenuAction>(
        value: .commitChanges,
        label: 'Commit Changes...',
        localizeLabel: false,
        leading: Icon(AleraIcons.gitCommit, size: 16),
      ),
    ],
  );
}

RelativeRect? _gitHistoryRefMenuPosition(
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
