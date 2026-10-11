import 'dart:convert';

import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_filename.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Creates a new UTF-8 text file in the current directory, never overwriting
/// an existing path and never following a directory symlink on the host.
class WorkspaceTextCreateScreen extends ConsumerStatefulWidget {
  const WorkspaceTextCreateScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.directory,
  });

  final String hostId;
  final String workspaceId;
  final String directory;

  @override
  ConsumerState<WorkspaceTextCreateScreen> createState() =>
      _WorkspaceTextCreateScreenState();
}

class _WorkspaceTextCreateScreenState
    extends ConsumerState<WorkspaceTextCreateScreen> {
  static final Logger _logger = Logger('WorkspaceTextCreateScreen');
  final _name = TextEditingController();
  final _text = TextEditingController();
  bool _saving = false;
  bool _discarding = false;

  bool get _dirty => _name.text.isNotEmpty || _text.text.isNotEmpty;

  @override
  void dispose() {
    _name.dispose();
    _text.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    if (_saving) return;
    final name = _name.text;
    if (!isSafeMobileWorkspaceFileName(name)) {
      _showError('Use one valid file name, without path separators.');
      return;
    }
    if (utf8.encode(_text.text).length > maxMobileWorkspaceTextEditBytes ||
        _text.text.contains('\u0000')) {
      _showError('Only UTF-8 text up to 256 KiB, without NUL characters.');
      return;
    }
    final relativePath = widget.directory.isEmpty
        ? name
        : '${widget.directory}/$name';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Create new file on host?'),
        content: Text(
          'Create $relativePath? A file with the same name will not be replaced.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Create'),
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
      await client.createWorkspaceText(
        workspaceId: widget.workspaceId,
        relativePath: relativePath,
        text: _text.text,
      );
      if (!mounted) return;
      setState(() => _discarding = true);
      Navigator.of(context).pop(relativePath);
    } on MobileRuntimeConflictException catch (error, trace) {
      _logger.warning('Mobile file creation conflict', error, trace);
      if (mounted) {
        _showError('This filename already exists on the host. Choose another.');
      }
    } on Object catch (error, trace) {
      _logger.warning('Mobile file creation failed', error, trace);
      if (mounted) _showError('Create failed: $error');
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
        title: const Text('Discard new file draft?'),
        content: const Text('The filename and unsaved text will be lost.'),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Keep editing'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Discard'),
          ),
        ],
      ),
    );
    if (discard == true && mounted) {
      setState(() => _discarding = true);
      Navigator.of(context).pop();
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
        title: const Text('New text file'),
        actions: <Widget>[
          if (_saving)
            const Padding(
              padding: AleraTokens.contentPadding,
              child: CircularProgressIndicator(),
            )
          else
            IconButton(
              tooltip: 'Create text file',
              icon: const Icon(Icons.note_add_outlined),
              onPressed: _create,
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
                'Folder: /${widget.directory} · no overwrites · max 256 KiB',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _name,
                readOnly: _saving,
                textInputAction: TextInputAction.next,
                decoration: const InputDecoration(
                  labelText: 'File name',
                  hintText: 'notes.md',
                  border: OutlineInputBorder(),
                ),
                onChanged: (_) => setState(() {}),
              ),
              const SizedBox(height: 12),
              Expanded(
                child: TextField(
                  controller: _text,
                  expands: true,
                  maxLines: null,
                  minLines: null,
                  readOnly: _saving,
                  keyboardType: TextInputType.multiline,
                  textAlignVertical: TextAlignVertical.top,
                  style: Theme.of(context).textTheme.bodyMedium
                      ?.copyWith(fontFamily: AleraTokens.monoFontFamily),
                  decoration: const InputDecoration(
                    labelText: 'Text content',
                    alignLabelWithHint: true,
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
