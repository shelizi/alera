import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/material.dart';

enum GitHistoryRefMenuAction { switchBranch, copyName }

enum GitHistoryCommitMenuAction { copyHash, copySubject }

/// Shows the actions available for a history reference at a global pointer
/// position. Unsupported reference kinds do not open an empty menu.
Future<GitHistoryRefMenuAction?> showGitHistoryRefMenu(
  BuildContext context,
  GitHistoryItemRef itemRef,
  Offset globalPosition, {
  required bool isCurrentBranch,
}) async {
  final kind = _gitHistoryRefMenuKind(itemRef);
  if (kind == _GitHistoryRefMenuKind.unsupported) {
    return null;
  }
  final position = _gitHistoryMenuPosition(context, globalPosition);
  if (position == null) {
    return null;
  }
  return showMenu<GitHistoryRefMenuAction>(
    context: context,
    position: position,
    items: <PopupMenuEntry<GitHistoryRefMenuAction>>[
      if (kind == _GitHistoryRefMenuKind.localBranch)
        AleraDropdownEntry<GitHistoryRefMenuAction>(
          value: .switchBranch,
          label: isCurrentBranch ? 'Current Branch' : 'Switch to Branch',
          localizeLabel: false,
          selected: isCurrentBranch,
          enabled: !isCurrentBranch,
          leading: const Icon(AleraIcons.gitBranch, size: 16),
        ),
      const AleraDropdownEntry<GitHistoryRefMenuAction>(
        value: .copyName,
        label: 'Copy Branch Name',
        localizeLabel: false,
        leading: Icon(AleraIcons.copy, size: 16),
      ),
    ],
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
    ],
  );
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

enum _GitHistoryRefMenuKind { localBranch, copyOnly, unsupported }

_GitHistoryRefMenuKind _gitHistoryRefMenuKind(GitHistoryItemRef itemRef) {
  final category = itemRef.category;
  if (category == GitHistoryRefCategory.branches ||
      (category == null && itemRef.id.startsWith('refs/heads/'))) {
    return _GitHistoryRefMenuKind.localBranch;
  }
  if (category == GitHistoryRefCategory.remoteBranches ||
      category == GitHistoryRefCategory.tags ||
      (category == null &&
          (itemRef.id.startsWith('refs/remotes/') ||
              itemRef.id.startsWith('refs/tags/')))) {
    return _GitHistoryRefMenuKind.copyOnly;
  }
  return _GitHistoryRefMenuKind.unsupported;
}
