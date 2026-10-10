import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_picker_sheet.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_diff_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
  late final Future<MobileWorkspaceTextPreview> _preview;

  @override
  void initState() {
    super.initState();
    _preview = _readPreview();
  }

  Future<MobileWorkspaceTextPreview> _readPreview() async {
    final client = await ref.read(
      hostConnectionControllerProvider(widget.hostId).future,
    );
    return loadWorkspaceTextPreview(
      (offset, length) => client.readWorkspaceFile(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
        offset: offset,
        length: length,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final canDiff =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceFileDiff ==
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
                      child: SelectableText(
                        preview.text,
                        style: Theme.of(context).textTheme.bodySmall
                            ?.copyWith(fontFamily: AleraTokens.monoFontFamily),
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
