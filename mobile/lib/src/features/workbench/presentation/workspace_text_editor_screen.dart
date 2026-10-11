import 'dart:convert';

import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Edits only a fully downloaded UTF-8 file, and saves with the original
/// SHA-256 precondition so a simultaneous desktop/agent change isn't lost.
class WorkspaceTextEditorScreen extends ConsumerStatefulWidget {
  const WorkspaceTextEditorScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.relativePath,
    required this.originalText,
    required this.originalSha256,
  });

  final String hostId;
  final String workspaceId;
  final String relativePath;
  final String originalText;
  final String originalSha256;

  @override
  ConsumerState<WorkspaceTextEditorScreen> createState() =>
      _WorkspaceTextEditorScreenState();
}

class _WorkspaceTextEditorScreenState
    extends ConsumerState<WorkspaceTextEditorScreen> {
  static final Logger _logger = Logger('WorkspaceTextEditor');
  late final TextEditingController _editor;
  bool _saving = false;
  bool _discarding = false;

  bool get _dirty => _editor.text != widget.originalText;

  @override
  void initState() {
    super.initState();
    _editor = TextEditingController(text: widget.originalText);
  }

  @override
  void dispose() {
    _editor.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_saving || !_dirty) return;
    if (utf8.encode(_editor.text).length > maxMobileWorkspaceTextEditBytes) {
      _showError('Edits must remain below 256 KiB.');
      return;
    }
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Save changes to host?'),
        content: Text(
          'Save ${widget.relativePath}? If another agent or editor changed the file, saving will be refused.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _saving = true);
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      await client.saveWorkspaceText(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
        expectedSha256: widget.originalSha256,
        text: _editor.text,
      );
      if (!mounted) return;
      setState(() => _discarding = true);
      Navigator.of(context).pop(true);
    } on MobileRuntimeConflictException catch (error, trace) {
      _logger.warning('Concurrent workspace edit detected', error, trace);
      if (mounted) {
        _showError(
          'This file changed on the host. Copy your draft and reopen it before saving.',
        );
      }
    } on Object catch (error, trace) {
      _logger.warning('Could not save mobile text edit', error, trace);
      if (mounted) _showError('Save failed: $error');
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  void _showError(String message) {
    ScaffoldMessenger.of(context)
        .showSnackBar(SnackBar(content: Text(message)));
  }

  Future<void> _confirmDiscard() async {
    final discard = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Discard edits?'),
        content: const Text('Unsaved changes will be lost.'),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Keep editing'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Discard'),
          ),
        ],
      ),
    );
    if (discard == true && mounted) {
      setState(() => _discarding = true);
      Navigator.of(context).pop(false);
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !_dirty || _discarding,
    onPopInvokedWithResult: (didPop, _) {
      if (!didPop && mounted && !_saving) _confirmDiscard();
    },
    child: Scaffold(
      appBar: AppBar(
        title: Text(
          'Edit ${widget.relativePath}',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        actions: <Widget>[
          if (_saving)
            const Padding(
              padding: AleraTokens.contentPadding,
              child: CircularProgressIndicator(),
            )
          else
            IconButton(
              tooltip: 'Save file',
              icon: const Icon(Icons.save_outlined),
              onPressed: _dirty ? _save : null,
            ),
        ],
      ),
      body: SafeArea(
        child: Padding(
          padding: AleraTokens.contentPadding,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Text(
                'Existing UTF-8 file · 256 KiB maximum · conflict protected',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 8),
              Expanded(
                child: TextField(
                  controller: _editor,
                  expands: true,
                  maxLines: null,
                  minLines: null,
                  readOnly: _saving,
                  keyboardType: TextInputType.multiline,
                  textAlignVertical: TextAlignVertical.top,
                  style: Theme.of(context).textTheme.bodyMedium
                      ?.copyWith(fontFamily: AleraTokens.monoFontFamily),
                  decoration: const InputDecoration(
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (_) => setState(() {}),
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}
