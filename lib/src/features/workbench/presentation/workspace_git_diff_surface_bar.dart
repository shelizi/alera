part of 'workspace_git_diff_surface.dart';

class const _GitDiffBar({
  required final String title,
  required final String? filePath,
  required final VoidCallback onRefresh,
  required final VoidCallback? onOpenFile,
  required final bool aiAssistEnabled,
  required final bool readingDiffReady,
  required final bool showingReadingDiff,
  required final bool readingDiffBusy,
  required final VoidCallback? onGenerateReadingDiff,
  required final VoidCallback? onRegenerateReadingDiff,
  required final VoidCallback? onCancelReadingDiff,
  required final VoidCallback? onToggleReadingDiff,
  required final GitDiffContentMode contentMode,
  required final VoidCallback onToggleContentMode,
  required final GitDiffPresentationMode presentationMode,
  required final VoidCallback onTogglePresentationMode,
  required final WorkspaceTextEncodingSelection encodingSelection,
  required final native.WorkspaceTextEncoding? detectedEncoding,
  required final ValueChanged<WorkspaceTextEncodingSelection>
  onEncodingSelected,
  required final GitDiffWhitespaceMode whitespaceMode,
  required final ValueChanged<GitDiffWhitespaceMode> onWhitespaceModeSelected,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
        child: Row(
          children: <Widget>[
            AleraFileIcon(pathOrName: filePath ?? title, kind: .file, size: 16),
            const SizedBox(width: AleraTokens.space8),
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: .ellipsis,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                  fontFamily: 'JetBrains Mono',
                ),
              ),
            ),
            if (aiAssistEnabled ||
                readingDiffReady ||
                readingDiffBusy) ...<Widget>[
              AleraIconButton(
                tooltip: readingDiffBusy
                    ? 'Cancel Reading Diff'
                    : readingDiffReady
                    ? showingReadingDiff
                          ? 'Show Original Diff'
                          : 'Show Reading Diff'
                    : 'Generate Reading Diff',
                icon: readingDiffBusy
                    ? AleraIcons.cancel
                    : showingReadingDiff
                    ? AleraIcons.diff
                    : AleraIcons.ai,
                onPressed: readingDiffBusy
                    ? onCancelReadingDiff
                    : readingDiffReady
                    ? onToggleReadingDiff
                    : onGenerateReadingDiff,
              ),
              if (aiAssistEnabled &&
                  readingDiffReady &&
                  !readingDiffBusy) ...<Widget>[
                const SizedBox(width: AleraTokens.space2),
                AleraIconButton(
                  tooltip: 'Regenerate Reading Diff',
                  icon: AleraIcons.refresh,
                  onPressed: onRegenerateReadingDiff,
                ),
              ],
              const SizedBox(width: AleraTokens.space2),
            ],
            PopupMenuButton<WorkspaceTextEncodingSelection>(
              tooltip: 'Diff Encoding',
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
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                    fontFamily: 'JetBrains Mono',
                  ),
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space2),
            PopupMenuButton<GitDiffWhitespaceMode>(
              tooltip: 'Whitespace Comparison',
              onSelected: onWhitespaceModeSelected,
              itemBuilder: (context) => <PopupMenuEntry<GitDiffWhitespaceMode>>[
                for (final mode in GitDiffWhitespaceMode.values)
                  PopupMenuItem<GitDiffWhitespaceMode>(
                    value: mode,
                    child: Text(mode.label),
                  ),
              ],
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: AleraTokens.space6,
                ),
                child: Text(
                  whitespaceMode.label,
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                    fontFamily: 'JetBrains Mono',
                  ),
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: contentMode == GitDiffContentMode.fullFile
                  ? context.tr('Switch to Diff Only')
                  : context.tr('Switch to Full File View'),
              icon: contentMode == GitDiffContentMode.fullFile
                  ? AleraIcons.diff
                  : AleraIcons.file,
              onPressed: onToggleContentMode,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: presentationMode == GitDiffPresentationMode.sideBySide
                  ? context.tr('Switch to Single-Column View')
                  : context.tr('Switch to Side-by-Side View'),
              icon: presentationMode == GitDiffPresentationMode.sideBySide
                  ? AleraIcons.diffUnified
                  : AleraIcons.diffSideBySide,
              onPressed: onTogglePresentationMode,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: onOpenFile == null
                  ? 'File is not available in working tree'
                  : 'Open file',
              icon: AleraIcons.external,
              onPressed: onOpenFile,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: 'Refresh',
              icon: AleraIcons.refresh,
              onPressed: onRefresh,
            ),
          ],
        ),
      ),
    );
  }
}
