import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:flutter/material.dart';

class const GitHistoryTagInput({
  required final String name,
  final String? message,
});

Future<String?> showGitHistoryBranchInputDialog(BuildContext context) {
  return showDialog<String>(
    context: context,
    builder: (_) => const _GitHistorySingleInputDialog(
      title: 'Create Branch Here',
      labelText: 'Branch Name',
      confirmLabel: 'Create Branch',
    ),
  );
}

Future<GitHistoryTagInput?> showGitHistoryTagInputDialog(BuildContext context) {
  return showDialog<GitHistoryTagInput>(
    context: context,
    builder: (_) => const _GitHistoryTagInputDialog(),
  );
}

Future<String?> showGitHistoryArchiveInputDialog(
  BuildContext context, {
  required String initialPath,
}) {
  return showDialog<String>(
    context: context,
    builder: (_) => _GitHistorySingleInputDialog(
      title: 'Create Archive',
      labelText: 'Archive Path',
      confirmLabel: 'Create Archive',
      initialValue: initialPath,
    ),
  );
}

class const _GitHistorySingleInputDialog({
  required this.title,
  required this.labelText,
  required this.confirmLabel,
  this.initialValue = '',
}) extends StatefulWidget {
  final String title;
  final String labelText;
  final String confirmLabel;
  final String initialValue;

  @override
  State<_GitHistorySingleInputDialog> createState() =>
      _GitHistorySingleInputDialogState();
}

class _GitHistorySingleInputDialogState
    extends State<_GitHistorySingleInputDialog> {
  late final TextEditingController _controller;
  String? _errorText;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initialValue);
    if (widget.initialValue.isNotEmpty) {
      _controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: widget.initialValue.length,
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final value = _controller.text.trim();
    if (value.isEmpty) {
      setState(() => _errorText = '${widget.labelText} is required');
      return;
    }
    Navigator.of(context).pop(value);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AleraDialog(
      maxWidth: 420,
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space20),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          children: <Widget>[
            Text(widget.title, style: theme.textTheme.titleMedium),
            const SizedBox(height: AleraTokens.space16),
            AleraTextField(
              controller: _controller,
              autofocus: true,
              labelText: widget.labelText,
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
                  child: Text(widget.confirmLabel),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _GitHistoryTagInputDialog extends StatefulWidget {
  const _GitHistoryTagInputDialog();

  @override
  State<_GitHistoryTagInputDialog> createState() =>
      _GitHistoryTagInputDialogState();
}

class _GitHistoryTagInputDialogState extends State<_GitHistoryTagInputDialog> {
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _messageController = TextEditingController();
  String? _errorText;

  @override
  void dispose() {
    _nameController.dispose();
    _messageController.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _nameController.text.trim();
    if (name.isEmpty) {
      setState(() => _errorText = 'Tag name is required');
      return;
    }
    final message = _messageController.text.trim();
    Navigator.of(context).pop(
      GitHistoryTagInput(name: name, message: message.isEmpty ? null : message),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AleraDialog(
      maxWidth: 420,
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space20),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          children: <Widget>[
            Text('Add Tag', style: theme.textTheme.titleMedium),
            const SizedBox(height: AleraTokens.space16),
            AleraTextField(
              controller: _nameController,
              autofocus: true,
              labelText: 'Tag Name',
              errorText: _errorText,
              onChanged: (_) {
                if (_errorText != null) {
                  setState(() => _errorText = null);
                }
              },
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: AleraTokens.space12),
            AleraTextField(
              controller: _messageController,
              labelText: 'Message (Optional)',
              minLines: 2,
              maxLines: 4,
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
                FilledButton(onPressed: _submit, child: const Text('Add Tag')),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
