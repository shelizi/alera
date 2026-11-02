import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_merge_resolution.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_providers.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_service.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:alera/src/rust/api/git/git_merge_ops.dart' as git;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

Future<void> showWorkspaceGitMergeResolutionDialog(
  BuildContext context, {
  required String path,
}) => showDialog<void>(
  context: context,
  builder: (_) => _MergeDialog(path: path),
);

class _MergeDialog extends ConsumerStatefulWidget {
  const _MergeDialog({required this.path});
  final String path;
  @override
  ConsumerState<_MergeDialog> createState() => _MergeDialogState();
}

class _MergeDialogState extends ConsumerState<_MergeDialog> {
  List<String> files = const [];
  String? selected;
  git.GitMergeConflictPreview? preview;
  bool busy = false;
  bool generating = false;
  String? suggestedFile;
  AiAssistMergeSuggestion? suggestion;
  String? modelLabel;
  int generation = 0;
  late final AiAssistService _service;
  String? error;

  @override
  void dispose() {
    _service.cancel(widget.path, .mergeConflicts);
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    _service = ref.read(aiAssistServiceProvider);
    refresh();
  }

  Future<void> refresh() async {
    setState(() => busy = true);
    try {
      final next = await git.mergeConflictedPaths(path: widget.path);
      final file = next.contains(selected) ? selected : next.firstOrNull;
      final result = file == null
          ? null
          : await git.mergeConflictPreview(path: widget.path, filePath: file);
      if (mounted)
        setState(() {
          files = next;
          selected = file;
          if (suggestedFile != file || preview != result) suggestion = null;
          preview = result;
          error = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> pick(String file) async {
    setState(() {
      selected = file;
      suggestion = null;
      busy = true;
    });
    try {
      final result = await git.mergeConflictPreview(
        path: widget.path,
        filePath: file,
      );
      if (mounted) setState(() => preview = result);
    } catch (e) {
      if (mounted) setState(() => error = e.toString());
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> accept(bool ours) async {
    final file = selected;
    if (file == null || busy || generating) return;
    setState(() => busy = true);
    try {
      await git.resolveMergeConflict(
        path: widget.path,
        filePath: file,
        ours: ours,
      );
      if (mounted) await refresh();
    } catch (e) {
      if (mounted)
        setState(() {
          error = e.toString();
          busy = false;
        });
    }
  }

  Future<void> generateSuggestion() async {
    final file = selected;
    final snapshot = preview;
    final settings = ref.read(settingsControllerProvider).aiAssist;
    if (generating ||
        busy ||
        !settings.enabled ||
        file == null ||
        snapshot == null ||
        snapshot.isBinary)
      return;
    final token = ++generation;
    setState(() {
      generating = true;
      suggestion = null;
      error = null;
    });
    try {
      final output = await _service.generate(
        AiAssistRequest(
          operation: .mergeConflicts,
          workspacePath: widget.path,
          settings: settings,
          mergeConflict: AiAssistMergeConflictContext(
            filePath: file,
            base: snapshot.base,
            ours: snapshot.ours,
            theirs: snapshot.theirs,
          ),
        ),
      );
      if (!mounted ||
          generation != token ||
          selected != file ||
          preview != snapshot)
        return;
      final parsed = parseMergeConflictsSuggestion(output.text);
      setState(() {
        suggestion = parsed;
        suggestedFile = file;
        modelLabel = output.agentLabel;
      });
    } catch (e) {
      if (mounted && generation == token) setState(() => error = e.toString());
    } finally {
      if (mounted && generation == token) setState(() => generating = false);
    }
  }

  void cancelSuggestion() {
    generation++;
    _service.cancel(widget.path, .mergeConflicts);
    setState(() => generating = false);
  }

  Future<void> reviewSuggestion() async {
    final current = suggestion;
    if (current == null || suggestedFile != selected) return;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.tr('AI Merge Suggestion')),
        content: SizedBox(
          width: 730,
          height: 470,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                modelLabel ?? '',
                style: Theme.of(context).textTheme.labelSmall,
              ),
              Text(current.explanation),
              const Divider(),
              Expanded(
                child: SingleChildScrollView(
                  child: SelectableText(
                    current.merged,
                    style: const TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 12,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Text(
                context.tr(
                  'Suggestion only. No files or Git index have been changed.',
                ),
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () async {
              await Clipboard.setData(ClipboardData(text: current.merged));
              if (context.mounted) Navigator.pop(context);
            },
            child: Text(context.tr('Copy Merged Text')),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(context.tr('Close')),
          ),
        ],
      ),
    );
  }

  Future<void> abort() async {
    final yes = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.tr('Abort Merge?')),
        content: Text(context.tr('Restore the pre-merge state?')),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.tr('Cancel')),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.tr('Abort Merge')),
          ),
        ],
      ),
    );
    if (yes != true || !mounted) return;
    setState(() => busy = true);
    try {
      await git.abortMergeInProgress(path: widget.path);
      if (mounted) Navigator.pop(context);
    } catch (e) {
      if (mounted)
        setState(() {
          error = e.toString();
          busy = false;
        });
    }
  }

  @override
  Widget build(BuildContext context) {
    final data = preview;
    return AlertDialog(
      title: Text(context.tr('Resolve Merge Conflicts')),
      content: SizedBox(
        width: 840,
        height: 500,
        child: Column(
          children: [
            Text(files.length.toString() + ' unresolved files'),
            if (error != null)
              Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            Expanded(
              child: Row(
                children: [
                  SizedBox(
                    width: 230,
                    child: ListView(
                      children: [
                        for (final file in files)
                          ListTile(
                            selected: selected == file,
                            title: Text(
                              file,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            onTap: busy ? null : () => pick(file),
                          ),
                      ],
                    ),
                  ),
                  const VerticalDivider(),
                  Expanded(
                    child: data == null
                        ? Center(
                            child: Text(context.tr('No unresolved conflicts')),
                          )
                        : data.isBinary
                        ? Center(
                            child: Text(
                              context.tr(
                                'Binary, symlink and submodule conflicts require manual resolution',
                              ),
                            ),
                          )
                        : Column(
                            children: [
                              _Version(label: 'Base', content: data.base),
                              _Version(
                                label: 'Current (ours)',
                                content: data.ours,
                              ),
                              _Version(
                                label: 'Incoming (theirs)',
                                content: data.theirs,
                              ),
                            ],
                          ),
                  ),
                ],
              ),
            ),
            if (busy || generating) const LinearProgressIndicator(),
            Row(
              children: [
                OutlinedButton.icon(
                  onPressed:
                      busy ||
                          generating ||
                          selected == null ||
                          data?.isBinary == true ||
                          !ref
                              .watch(settingsControllerProvider)
                              .aiAssist
                              .enabled
                      ? null
                      : generateSuggestion,
                  icon: const Icon(Icons.auto_awesome, size: 16),
                  label: Text(context.tr('Suggest Merge with AI')),
                ),
                if (generating)
                  TextButton(
                    onPressed: cancelSuggestion,
                    child: Text(context.tr('Cancel')),
                  ),
                if (suggestion != null && suggestedFile == selected)
                  TextButton(
                    onPressed: reviewSuggestion,
                    child: Text(context.tr('Review AI Suggestion')),
                  ),
              ],
            ),
            Row(
              children: [
                OutlinedButton(
                  onPressed: busy || selected == null || data?.isBinary == true
                      ? null
                      : () => accept(true),
                  child: Text(context.tr('Accept Current')),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  onPressed: busy || selected == null || data?.isBinary == true
                      ? null
                      : () => accept(false),
                  child: Text(context.tr('Accept Incoming')),
                ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: busy ? null : abort,
          child: Text(context.tr('Abort Merge')),
        ),
        TextButton(
          onPressed: busy ? null : refresh,
          child: Text(context.tr('Refresh')),
        ),
        FilledButton(
          onPressed: busy ? null : () => Navigator.pop(context),
          child: Text(context.tr('Done')),
        ),
      ],
    );
  }
}

class _Version extends StatelessWidget {
  const _Version({required this.label, required this.content});
  final String label;
  final String? content;
  @override
  Widget build(BuildContext context) => Expanded(
    child: Card(
      child: Column(
        children: [
          Text(label, style: Theme.of(context).textTheme.labelMedium),
          Expanded(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(8),
              child: SelectableText(
                content ?? '(deleted)',
                style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
              ),
            ),
          ),
        ],
      ),
    ),
  );
}
