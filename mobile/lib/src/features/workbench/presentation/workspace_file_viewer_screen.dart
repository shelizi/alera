import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_markdown_preview.dart';
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
  bool _markdownPreview = true;

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
                  const Expanded(
                    child: Center(
                      child: Text('Binary files cannot be previewed.'),
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
