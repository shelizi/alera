part of 'workspace_editor_surface.dart';

@visibleForTesting
String workspaceEditorCodeForgeKey({
  required String tabId,
  required String filePath,
  required String themeName,
}) {
  return 'workspace-editor-$tabId-$filePath-$themeName';
}

class const _EditorFileBar({
  required final String path,
  required final bool dirty,
  required final bool saving,
  required final WorkspaceTextEncodingSelection encodingSelection,
  required final native.WorkspaceTextEncoding? detectedEncoding,
  required final ValueChanged<WorkspaceTextEncodingSelection>?
  onEncodingSelected,
  required final VoidCallback? onViewDiff,
  required final VoidCallback? onSave,
  required final VoidCallback? onDiscard,
  required final VoidCallback? onOpenPreview,
  required final bool outlineOpen,
  required final VoidCallback? onToggleOutline,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = dirty ? AleraTokens.foreground : AleraTokens.foregroundMuted;
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
        child: Row(
          children: <Widget>[
            AleraFileIcon(
              pathOrName: path,
              kind: .file,
              size: 16,
              fallbackColor: color,
            ),
            const SizedBox(width: AleraTokens.space8),
            Expanded(
              child: Text(
                path,
                maxLines: 1,
                overflow: .ellipsis,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: color,
                  fontFamily: 'JetBrains Mono',
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space8),
            PopupMenuButton<WorkspaceTextEncodingSelection>(
              tooltip: 'File Encoding',
              enabled: onEncodingSelected != null,
              onSelected: onEncodingSelected,
              itemBuilder: (context) =>
                  <PopupMenuEntry<WorkspaceTextEncodingSelection>>[
                    for (final selection
                        in WorkspaceTextEncodingSelection.values)
                      PopupMenuItem<WorkspaceTextEncodingSelection>(
                        value: selection,
                        child: Text(selection.label),
                      ),
                  ],
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: AleraTokens.space6,
                ),
                child: Text(
                  workspaceTextEncodingDisplayLabel(
                    selection: encodingSelection,
                    detectedEncoding: detectedEncoding,
                  ),
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                    fontFamily: 'JetBrains Mono',
                  ),
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: outlineOpen ? 'Hide Outline' : 'Show Outline',
              icon: AleraIcons.outline,
              onPressed: onToggleOutline,
              iconColor: outlineOpen ? AleraTokens.foreground : color,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: 'View Diff',
              icon: AleraIcons.diff,
              onPressed: onViewDiff,
              iconColor: color,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: saving ? 'Saving File' : 'Save File',
              icon: saving ? AleraIcons.loading : AleraIcons.save,
              onPressed: onSave,
              iconColor: color,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: 'Discard Changes',
              icon: AleraIcons.restore,
              onPressed: onDiscard,
              iconColor: color,
            ),
            if (onOpenPreview != null) ...<Widget>[
              const SizedBox(width: AleraTokens.space2),
              AleraIconButton(
                tooltip: 'Open Preview',
                icon: AleraIcons.preview,
                onPressed: onOpenPreview,
                iconColor: color,
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class const _EditorOutlinePanel({
  required final bool loading,
  required final List<code_forge.CodeForgeDocumentSymbol> symbols,
  required final bool truncated,
  required final code_forge.CodeForgeDocumentSymbolSource? source,
  required final VoidCallback? onRefresh,
  required final ValueChanged<code_forge.CodeForgeDocumentSymbol> onSelect,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final sourceLabel = switch (source) {
      code_forge.CodeForgeDocumentSymbolSource.lsp => 'LSP',
      code_forge.CodeForgeDocumentSymbolSource.native => 'Tree-sitter',
      null => null,
    };

    return ColoredBox(
      color: AleraTokens.bg,
      child: Column(
        crossAxisAlignment: .stretch,
        children: <Widget>[
          SizedBox(
            height: AleraTokens.sidebarHeaderHeight,
            child: Padding(
              padding: const EdgeInsets.only(
                left: AleraTokens.space12,
                right: AleraTokens.space4,
              ),
              child: Row(
                children: <Widget>[
                  const Icon(
                    AleraIcons.outline,
                    size: 14,
                    color: AleraTokens.foregroundMuted,
                  ),
                  const SizedBox(width: AleraTokens.space6),
                  Expanded(
                    child: Text(
                      'Outline',
                      style: theme.textTheme.labelMedium?.copyWith(
                        color: AleraTokens.foreground,
                      ),
                    ),
                  ),
                  if (sourceLabel != null)
                    Text(
                      sourceLabel,
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: AleraTokens.foregroundMuted,
                      ),
                    ),
                  AleraIconButton(
                    tooltip: 'Refresh Outline',
                    icon: AleraIcons.refresh,
                    onPressed: loading ? null : onRefresh,
                    iconColor: AleraTokens.foregroundMuted,
                  ),
                ],
              ),
            ),
          ),
          const Divider(height: 1, color: AleraTokens.borderSubtle),
          if (loading && symbols.isEmpty)
            const Expanded(
              child: Center(
                child: SizedBox.square(
                  dimension: AleraTokens.space16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
            )
          else if (symbols.isEmpty)
            Expanded(
              child: Center(
                child: Text(
                  'No symbols',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                  ),
                ),
              ),
            )
          else ...<Widget>[
            if (loading) const LinearProgressIndicator(minHeight: 1),
            if (truncated)
              Padding(
                padding: const EdgeInsets.fromLTRB(
                  AleraTokens.space12,
                  AleraTokens.space6,
                  AleraTokens.space12,
                  AleraTokens.space4,
                ),
                child: Text(
                  'Showing the first 1000 symbols',
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                  ),
                ),
              ),
            Expanded(
              child: ListView.builder(
                itemCount: symbols.length,
                itemExtent: 30,
                itemBuilder: (context, index) {
                  final symbol = symbols[index];
                  return InkWell(
                    onTap: () => onSelect(symbol),
                    child: Padding(
                      padding: EdgeInsets.only(
                        left: AleraTokens.space8 + symbol.depth * 12.0,
                        right: AleraTokens.space8,
                      ),
                      child: Row(
                        children: <Widget>[
                          Expanded(
                            child: Text(
                              symbol.name,
                              maxLines: 1,
                              overflow: .ellipsis,
                              style: theme.textTheme.bodySmall?.copyWith(
                                color: AleraTokens.foreground,
                                fontFamily: 'JetBrains Mono',
                              ),
                            ),
                          ),
                          const SizedBox(width: AleraTokens.space6),
                          Text(
                            symbol.kind,
                            maxLines: 1,
                            overflow: .ellipsis,
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: AleraTokens.foregroundMuted,
                            ),
                          ),
                        ],
                      ),
                    ),
                  );
                },
              ),
            ),
          ],
        ],
      ),
    );
  }
}
