import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_image_preview.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_markdown_preview.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_filename.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_picker_sheet.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_diff_screen.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_text_editor_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

class const WorkspaceFileViewerScreen({
  super.key,
  required final String hostId,
  required final String workspaceId,
  required final String relativePath,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceFileViewerScreen> createState() =>
      _WorkspaceFileViewerScreenState();
}

class _WorkspaceFileViewerScreenState
    extends ConsumerState<WorkspaceFileViewerScreen> {
  static final Logger _logger = Logger('WorkspaceFileViewerScreen');
  late Future<MobileWorkspaceTextPreview> _preview;
  Future<MobileWorkspaceImagePreview>? _imagePreview;
  bool _markdownPreview = true;
  bool _renaming = false;
  bool _trashing = false;

  @override
  void initState() {
    super.initState();
    _preview = _readPreview();
  }

  Future<MobileWorkspaceTextPreview> _readPreview() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await loadWorkspaceTextPreview(
        (offset, length) => client.readWorkspaceFile(
          workspaceId: widget.workspaceId,
          relativePath: widget.relativePath,
          offset: offset,
          length: length,
        ),
      );
    } on Object catch (error, stackTrace) {
      _logger.warning(
        'Could not preview mobile workspace file',
        error,
        stackTrace,
      );
      rethrow;
    }
  }

  Future<MobileWorkspaceImagePreview> _readImage() async {
    final client = await ref.read(
      hostConnectionControllerProvider(widget.hostId).future,
    );
    return loadWorkspaceImagePreview(
      (offset, length) => client.readWorkspaceFile(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
        offset: offset,
        length: length,
      ),
    );
  }

  Future<void> _renameText(String expectedSha256) async {
    if (_renaming) return;
    final currentName = workspaceFileBaseName(widget.relativePath);
    var enteredName = currentName;
    final newName = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Rename text file on host?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            const Text(
              'Only the filename changes. Existing destinations are never overwritten.',
            ),
            const SizedBox(height: 12),
            TextFormField(
              initialValue: currentName,
              autofocus: true,
              onChanged: (text) => enteredName = text,
              decoration: const InputDecoration(
                labelText: 'New filename',
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(enteredName),
            child: const Text('Rename'),
          ),
        ],
      ),
    );
    if (!mounted || newName == null) return;
    if (newName == currentName || !isSafeMobileWorkspaceFileName(newName)) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Choose a different single filename.')),
      );
      return;
    }
    setState(() => _renaming = true);
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      await client.renameWorkspaceText(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
        newName: newName,
        expectedSha256: expectedSha256,
      );
      if (mounted) Navigator.of(context).pop();
    } on MobileRuntimeConflictException catch (error, trace) {
      _logger.warning('Mobile text rename conflicted', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'The file changed or the destination exists. Reopen and retry.',
            ),
          ),
        );
      }
    } on Object catch (error, trace) {
      _logger.warning('Mobile text rename failed', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Rename failed: $error')));
      }
    } finally {
      if (mounted) setState(() => _renaming = false);
    }
  }

  Future<void> _moveToTrash(String expectedSha256) async {
    if (_trashing || _renaming) return;
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Move text file to recovery storage?'),
        content: Text(
          '${widget.relativePath} will be removed from the workspace only after a verified backup is saved by the paired Runtime. You can restore it from Recoverable Files. Changes made after opening will block this action.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Move to recovery'),
          ),
        ],
      ),
    );
    if (accepted != true || !mounted) return;
    setState(() => _trashing = true);
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      await client.moveWorkspaceTextToTrash(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
        expectedSha256: expectedSha256,
      );
      if (mounted) Navigator.of(context).pop();
    } on MobileRuntimeConflictException catch (error, trace) {
      _logger.warning('Mobile trash conflict', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('File changed on host. Reload before moving it.'),
          ),
        );
      }
    } on Object catch (error, trace) {
      _logger.warning('Mobile trash failed', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Move to recovery failed: $error')),
        );
      }
    } finally {
      if (mounted) setState(() => _trashing = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final canDiff =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceFileDiff ==
        true;
    final canEdit =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceTextEdit ==
        true;
    final canRename =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceTextRename ==
        true;
    final canTrash =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceTextTrash ==
        true;
    return Scaffold(
      appBar: AppBar(
        title: Text(
          workspaceFileBaseName(widget.relativePath),
          overflow: TextOverflow.ellipsis,
        ),
        actions: <Widget>[
          if (canDiff)
            IconButton(
              tooltip: 'View Git Changes',
              icon: const Icon(Icons.difference_outlined),
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => WorkspaceFileDiffScreen(
                    hostId: widget.hostId,
                    workspaceId: widget.workspaceId,
                    relativePath: widget.relativePath,
                  ),
                ),
              ),
            ),
        ],
      ),
      body: SafeArea(
        child: FutureBuilder<MobileWorkspaceTextPreview>(
          future: _preview,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Center(
                child: Padding(
                  padding: AleraTokens.contentPadding,
                  child: Text('Unable to read file: ${snapshot.error}'),
                ),
              );
            }
            final preview = snapshot.data;
            if (preview == null) {
              return const Center(child: CircularProgressIndicator());
            }
            return Column(
              crossAxisAlignment: .stretch,
              children: <Widget>[
                Padding(
                  padding: AleraTokens.contentPadding,
                  child: Text(
                    '${preview.relativePath} (${preview.totalBytes} bytes)',
                    style: Theme.of(context).textTheme.bodySmall
                        ?.copyWith(color: AleraTokens.foregroundMuted),
                  ),
                ),
                if (canEdit && preview.originalSha256 != null)
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton.icon(
                      icon: const Icon(Icons.edit_outlined),
                      label: const Text('Edit text'),
                      onPressed: () async {
                        final saved = await Navigator.of(context).push<bool>(
                          MaterialPageRoute<bool>(
                            builder: (_) => WorkspaceTextEditorScreen(
                              hostId: widget.hostId,
                              workspaceId: widget.workspaceId,
                              relativePath: widget.relativePath,
                              originalText: preview.text,
                              originalSha256: preview.originalSha256!,
                            ),
                          ),
                        );
                        if (saved == true && mounted) {
                          setState(() => _preview = _readPreview());
                        }
                      },
                    ),
                  ),
                if (canRename && preview.originalSha256 != null)
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton.icon(
                      icon: const Icon(Icons.drive_file_rename_outline),
                      label: const Text('Rename file'),
                      onPressed: _renaming
                          ? null
                          : () => _renameText(preview.originalSha256!),
                    ),
                  ),
                if (canTrash && preview.originalSha256 != null)
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton.icon(
                      icon: const Icon(Icons.delete_outline),
                      label: const Text('Move to recovery'),
                      onPressed: (_trashing || _renaming)
                          ? null
                          : () => _moveToTrash(preview.originalSha256!),
                    ),
                  ),
                if (preview.isText &&
                    isMarkdownWorkspaceFile(widget.relativePath))
                  SwitchListTile.adaptive(
                    title: const Text('Formatted Markdown preview'),
                    subtitle: const Text(
                      'Passive text-only rendering; no remote images or scripts',
                    ),
                    value: _markdownPreview,
                    onChanged: (value) =>
                        setState(() => _markdownPreview = value),
                  ),
                const Divider(height: 1),
                if (!preview.isText)
                  Expanded(
                    child:
                        isSupportedMobileWorkspaceImageMime(preview.mimeType) &&
                            preview.totalBytes <=
                                maxMobileWorkspaceImagePreviewBytes
                        ? FutureBuilder<MobileWorkspaceImagePreview>(
                            future: _imagePreview ??= _readImage(),
                            builder: (context, imageSnapshot) {
                              if (imageSnapshot.hasError) {
                                return Center(
                                  child: Text(
                                    'Unable to preview image: ${imageSnapshot.error}',
                                  ),
                                );
                              }
                              final image = imageSnapshot.data;
                              if (image == null) {
                                return const Center(
                                  child: CircularProgressIndicator(),
                                );
                              }
                              return InteractiveViewer(
                                minScale: 0.5,
                                maxScale: 5,
                                child: Center(
                                  child: Image.memory(
                                    image.bytes,
                                    fit: BoxFit.contain,
                                    cacheWidth: 2048,
                                    cacheHeight: 2048,
                                    errorBuilder: (_, _, _) => const Text(
                                      'This image cannot be decoded.',
                                    ),
                                  ),
                                ),
                              );
                            },
                          )
                        : Center(
                            child: Text(
                              isSupportedMobileWorkspaceImageMime(
                                    preview.mimeType,
                                  )
                                  ? 'Image preview is limited to 4 MiB.'
                                  : 'This binary file type cannot be previewed.',
                            ),
                          ),
                  )
                else ...<Widget>[
                  if (preview.truncated)
                    const Padding(
                      padding: AleraTokens.contentPadding,
                      child: Text('Showing the first 512 KiB (read-only).'),
                    ),
                  Expanded(
                    child: SingleChildScrollView(
                      padding: AleraTokens.contentPadding,
                      child:
                          _markdownPreview &&
                              isMarkdownWorkspaceFile(widget.relativePath)
                          ? SelectableText.rich(
                              TextSpan(
                                children: <TextSpan>[
                                  for (final line in parsePassiveMarkdown(
                                    preview.text,
                                  ))
                                    TextSpan(
                                      text: '${line.text}\n',
                                      style: TextStyle(
                                        fontWeight: line.kind == .heading
                                            ? FontWeight.bold
                                            : FontWeight.normal,
                                        fontStyle: line.kind == .quote
                                            ? FontStyle.italic
                                            : FontStyle.normal,
                                        fontSize: line.kind == .heading
                                            ? (line.level <= 2 ? 19 : 15)
                                            : 13,
                                        fontFamily: line.kind == .code
                                            ? AleraTokens.monoFontFamily
                                            : null,
                                        color: line.kind == .code
                                            ? Theme.of(context)
                                                  .colorScheme
                                                  .primary
                                            : null,
                                      ),
                                    ),
                                ],
                              ),
                              style: Theme.of(context).textTheme.bodySmall,
                            )
                          : SelectableText(
                              preview.text,
                              style: Theme.of(context).textTheme.bodySmall
                                  ?.copyWith(
                                    fontFamily: AleraTokens.monoFontFamily,
                                  ),
                            ),
                    ),
                  ),
                ],
              ],
            );
          },
        ),
      ),
    );
  }
}
