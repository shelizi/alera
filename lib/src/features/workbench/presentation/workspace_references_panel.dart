import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_empty_state.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/workbench/application/workspace_references_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

class const WorkspaceReferencesPanel({
  super.key,
  required final Workspace workspace,
  final Future<void> Function(WorkspaceReferenceMatch match)? onOpenReference,
}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final provider = workspaceReferencesControllerProvider(workspace.id);
    final state = ref.watch(provider);
    final controller = ref.read(provider.notifier);
    final result = state.result;
    final rows = result == null
        ? const <_ReferencePanelRow>[]
        : _referencePanelRows(result);

    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        _ReferencesToolbar(
          state: state,
          onClear: state.loading || result != null || state.error != null
              ? controller.clear
              : null,
        ),
        Expanded(
          child: state.loading
              ? const Center(child: CircularProgressIndicator())
              : state.error != null
              ? _ReferencesError(message: state.error!)
              : result == null
              ? const AleraEmptyState(
                  icon: AleraIcons.listView,
                  title: 'Find References',
                  message: 'Run Find References from an editor to show project references here.',
                )
              : rows.isEmpty
              ? const AleraEmptyState(
                  icon: AleraIcons.listView,
                  title: 'No References',
                  message: 'No references were found in the current workspace.',
                )
              : ListView.builder(
                  itemCount: rows.length,
                  itemBuilder: (context, index) {
                    final row = rows[index];
                    return switch (row) {
                      _ReferenceFileRow(:final file) => _ReferenceFileHeader(
                        file: file,
                      ),
                      _ReferenceMatchRow(:final match) => _ReferenceMatchTile(
                        match: match,
                        onOpen: onOpenReference == null
                            ? null
                            : () => unawaited(onOpenReference!(match)),
                      ),
                    };
                  },
                ),
        ),
      ],
    );
  }
}

class const _ReferencesToolbar({
  required final WorkspaceReferencesState state,
  final VoidCallback? onClear,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final result = state.result;
    final total =
        result?.files.fold<int>(0, (sum, file) => sum + file.matches.length) ??
        0;
    final summary = result == null
        ? context.tr('References')
        : '$total ${context.tr(total == 1 ? 'reference' : 'references')} · '
              '${result.files.length} ${context.tr(result.files.length == 1 ? 'file' : 'files')}';
    return DecoratedBox(
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: SizedBox(
        height: AleraTokens.sidebarHeaderHeight,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
          child: Row(
            children: <Widget>[
              Expanded(
                child: Text(
                  summary,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                    fontWeight: .w600,
                  ),
                ),
              ),
              if (result?.truncated == true)
                Padding(
                  padding: const EdgeInsets.only(right: AleraTokens.space6),
                  child: Text(
                    context.tr('Partial results'),
                    style: Theme.of(context).textTheme.bodySmall
                        ?.copyWith(color: AleraTokens.foregroundFaint),
                  ),
                ),
              AleraIconButton(
                tooltip: 'Clear References',
                icon: AleraIcons.close,
                onPressed: onClear,
                minSize: 28,
                iconSize: 15,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class const _ReferencesError({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(AleraTokens.space16),
      child: Text(
        message,
        textAlign: .center,
        style: Theme.of(context).textTheme.bodySmall
            ?.copyWith(color: AleraTokens.error),
      ),
    ),
  );
}

sealed class _ReferencePanelRow {
  const _ReferencePanelRow();
}

final class _ReferenceFileRow extends _ReferencePanelRow {
  const _ReferenceFileRow(this.file);

  final WorkspaceReferenceFileGroup file;
}

final class _ReferenceMatchRow extends _ReferencePanelRow {
  const _ReferenceMatchRow(this.match);

  final WorkspaceReferenceMatch match;
}

List<_ReferencePanelRow> _referencePanelRows(WorkspaceReferencesResult result) {
  final rows = <_ReferencePanelRow>[];
  for (final file in result.files) {
    rows.add(_ReferenceFileRow(file));
    rows.addAll(file.matches.map(_ReferenceMatchRow.new));
  }
  return rows;
}

class const _ReferenceFileHeader({
  required final WorkspaceReferenceFileGroup file,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final name = p.basename(file.relativePath);
    final directory = p.dirname(file.relativePath);
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surfaceVariant,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          AleraTokens.space8,
          AleraTokens.space6,
          AleraTokens.space8,
          AleraTokens.space6,
        ),
        child: Row(
          children: <Widget>[
            AleraFileIcon(
              pathOrName: name,
              kind: .file,
              size: 15,
              fallbackColor: AleraTokens.foregroundMuted,
            ),
            const SizedBox(width: AleraTokens.space6),
            Expanded(
              child: RichText(
                maxLines: 1,
                overflow: .ellipsis,
                text: TextSpan(
                  children: <InlineSpan>[
                    TextSpan(
                      text: name,
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: AleraTokens.foreground,
                        fontWeight: .w600,
                      ),
                    ),
                    if (directory != '.')
                      TextSpan(
                        text: '  $directory',
                        style: Theme.of(context).textTheme.bodySmall
                            ?.copyWith(color: AleraTokens.foregroundMuted),
                      ),
                  ],
                ),
              ),
            ),
            Text(
              '${file.matches.length}',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: AleraTokens.foregroundMuted,
                fontWeight: .w600,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class const _ReferenceMatchTile({
  required final WorkspaceReferenceMatch match,
  final VoidCallback? onOpen,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) => InkWell(
    mouseCursor: onOpen == null ? null : SystemMouseCursors.click,
    onTap: onOpen,
    child: Padding(
      padding: const EdgeInsets.fromLTRB(
        AleraTokens.space8,
        AleraTokens.space6,
        AleraTokens.space8,
        AleraTokens.space6,
      ),
      child: Row(
        crossAxisAlignment: .start,
        children: <Widget>[
          SizedBox(
            width: AleraTokens.space32,
            child: Text(
              '${match.line}',
              textAlign: .right,
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.foregroundFaint),
            ),
          ),
          const SizedBox(width: AleraTokens.space8),
          Expanded(
            child: Text(
              match.linePreview.isEmpty
                  ? '${context.tr('Line')} ${match.line}'
                  : match.linePreview,
              maxLines: 2,
              overflow: .ellipsis,
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.foreground),
            ),
          ),
        ],
      ),
    ),
  );
}
