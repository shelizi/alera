import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/workbench/presentation/workbench_dialog_launchers.dart';
import 'package:flutter/material.dart';

Future<String?> showGitHistoryRenameBranchDialog(
  BuildContext context,
  String branch,
) {
  return showRenameDialog(
    context,
    title: 'Rename Branch',
    labelText: 'Branch Name',
    initialValue: branch,
    confirmLabel: 'Rename',
  );
}

Future<bool> showGitHistoryDeleteBranchConfirmation(
  BuildContext context,
  String branch,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Delete Branch?',
      message: 'Delete the local branch "$branch"?',
      confirmLabel: 'Delete',
      destructive: true,
    ),
  );
  return result ?? false;
}

Future<bool> showGitHistoryForceDeleteBranchConfirmation(
  BuildContext context,
  String branch,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Force Delete Branch?',
      message:
          'Branch "$branch" was not fully merged. Force deletion removes '
          'the branch reference and cannot be undone.',
      confirmLabel: 'Delete Forcefully',
      destructive: true,
    ),
  );
  return result ?? false;
}

Future<bool> showGitHistoryDeleteRemoteBranchConfirmation(
  BuildContext context, {
  required String remote,
  required String branch,
}) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Delete $branch on $remote?',
      message: 'This deletes the remote branch and cannot be undone.',
      confirmLabel: 'Delete',
      destructive: true,
    ),
  );
  return result ?? false;
}

Future<bool> showGitHistoryDeleteTagConfirmation(
  BuildContext context,
  String tag,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Delete Tag?',
      message: 'Delete the local tag "$tag"?',
      confirmLabel: 'Delete',
      destructive: true,
    ),
  );
  return result ?? false;
}

Future<bool> showGitHistoryMergeConfirmation(
  BuildContext context, {
  required String branch,
  required String currentBranch,
}) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Merge $branch into $currentBranch?',
      message: 'This creates a merge or fast-forward on the current branch.',
      confirmLabel: 'Merge',
    ),
  );
  return result ?? false;
}

Future<bool> showGitHistoryRebaseConfirmation(
  BuildContext context,
  String branch,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => AleraConfirmDialog(
      title: 'Rebase Current Branch onto $branch?',
      message: 'The current branch will be replayed onto "$branch".',
      confirmLabel: 'Rebase',
    ),
  );
  return result ?? false;
}

Future<String?> showGitHistoryArchivePathDialog(
  BuildContext context, {
  required String initialValue,
}) {
  return showDialog<String>(
    context: context,
    builder: (_) => _GitHistoryArchivePathDialog(initialValue: initialValue),
  );
}

Future<bool> showGitHistoryDiscardAllChangesConfirmation(
  BuildContext context,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (_) => const AleraConfirmDialog(
      title: 'Discard All Changes?',
      message:
          'This permanently discards all staged, unstaged and untracked '
          'changes in this workspace.',
      confirmLabel: 'Discard',
      destructive: true,
    ),
  );
  return result ?? false;
}

class const _GitHistoryArchivePathDialog({required this.initialValue})
    extends StatefulWidget {
  final String initialValue;

  @override
  State<_GitHistoryArchivePathDialog> createState() =>
      _GitHistoryArchivePathDialogState();
}

class _GitHistoryArchivePathDialogState
    extends State<_GitHistoryArchivePathDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initialValue,
  );
  String? _errorText;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final value = _controller.text.trim();
    if (value.isEmpty) {
      setState(() => _errorText = 'Output Path is required');
      return;
    }
    Navigator.of(context).pop(value);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AleraDialog(
      maxWidth: 460,
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space20),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          children: <Widget>[
            Text('Create Archive', style: theme.textTheme.titleMedium),
            const SizedBox(height: AleraTokens.space16),
            AleraTextField(
              controller: _controller,
              autofocus: true,
              labelText: 'Output Path',
              errorText: _errorText,
              onChanged: (_) {
                if (_errorText != null) {
                  setState(() => _errorText = null);
                }
              },
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: AleraTokens.space20),
            Row(
              mainAxisAlignment: .end,
              children: <Widget>[
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: AleraTokens.space8),
                FilledButton(
                  onPressed: _submit,
                  child: const Text('Create Archive'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
