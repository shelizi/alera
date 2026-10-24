part of 'workspace_workbench_view.dart';

/// Window-management actions a detached shared-engine chip exposes instead
/// of pane splits; both resolve against the child window hosting the tab.
final class WorkspaceTabDetachedMenuActions {
  const WorkspaceTabDetachedMenuActions({
    required this.onMoveToMainWindow,
    required this.onMoveToNewWindow,
  });

  /// Docks the tab back into the primary window strip.
  final Future<bool> Function() onMoveToMainWindow;

  /// Spawns a fresh detached native window hosting only this tab.
  final Future<bool> Function() onMoveToNewWindow;
}

extension _WorkspaceTabMenu on _WorkspaceTabChip {
  Future<void> _openContextMenu(
    BuildContext context,
    Offset globalPosition,
    WidgetRef ref,
  ) {
    return showWorkspaceTabContextMenu(
      context,
      globalPosition: globalPosition,
      ref: ref,
      workspace: workspace,
      sourceControlScope: sourceControlScope,
      tab: tab,
      terminalSession: terminalSession,
      groupTabs: groupTabs,
      onClose: onClose,
      onCloseTabs: onCloseTabs,
      onRename: onRename,
      onSplit: onSplit,
      onOpenExternalTerminal: onOpenExternalTerminal,
    );
  }
}

/// The chip context menu shared by the primary workbench strip and detached
/// shared-engine windows. [detachedActions] swaps the pane-split block for
/// dock/spawn-window entries inside a child HWND, and [onSplit] stays null
/// there because a child window has no pane model.
Future<void> showWorkspaceTabContextMenu(
  BuildContext context, {
  required Offset globalPosition,
  required WidgetRef ref,
  required Workspace workspace,
  required WorkspaceSourceControlScope? sourceControlScope,
  required WorkspaceTabRecord tab,
  required TerminalSessionHandle? terminalSession,
  required List<WorkspaceTabRecord> groupTabs,
  required VoidCallback onClose,
  required ValueChanged<List<String>> onCloseTabs,
  required ValueChanged<String> onRename,
  required ValueChanged<WorkbenchDropZone>? onSplit,
  OpenExternalTerminalCallback? onOpenExternalTerminal,
  WorkspaceTabDetachedMenuActions? detachedActions,
}) async {
  final overlay =
      Navigator.of(context).overlay!.context.findRenderObject()! as RenderBox;
  final tabIndex = groupTabs.indexWhere((candidate) => candidate.id == tab.id);
  final closeOthers = <String>[
    for (final candidate in groupTabs)
      if (candidate.id != tab.id && !candidate.isPinned) candidate.id,
  ];
  final closeRight = tabIndex < 0
      ? const <String>[]
      : <String>[
          for (final candidate in groupTabs.skip(tabIndex + 1))
            if (!candidate.isPinned) candidate.id,
        ];
  final filePath = tab.filePath;
  final canCompareWithGit =
      filePath != null &&
      switch (tab.kind) {
        WorkspaceTabKind.editor ||
        WorkspaceTabKind.markdownViewer ||
        WorkspaceTabKind.pdf => true,
        WorkspaceTabKind.gitDiff ||
        WorkspaceTabKind.gitHistory ||
        WorkspaceTabKind.terminal => false,
      };
  final workspaceFolderOpener = filePath == null
      ? null
      : ref.read(workspaceFolderOpenerProvider);
  final selected = await showMenu<_TabMenuAction>(
    context: context,
    position: .fromRect(
      .fromPoints(globalPosition, globalPosition),
      Offset.zero & overlay.size,
    ),
    items: <PopupMenuEntry<_TabMenuAction>>[
      if (detachedActions == null &&
          onSplit != null) ...<PopupMenuEntry<_TabMenuAction>>[
        const AleraDropdownEntry<_TabMenuAction>(
          value: .splitUp,
          label: 'Split Up',
          leading: _SplitDirectionGlyph(zone: .up),
        ),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .splitDown,
          label: 'Split Down',
          leading: _SplitDirectionGlyph(zone: .down),
        ),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .splitLeft,
          label: 'Split Left',
          leading: _SplitDirectionGlyph(zone: .left),
        ),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .splitRight,
          label: 'Split Right',
          leading: _SplitDirectionGlyph(zone: .right),
        ),
        const PopupMenuDivider(height: AleraTokens.space8),
      ],
      if (detachedActions != null)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .moveToMainWindow,
          label: 'Move to Main Window',
          leading: Icon(Icons.call_merge, size: 16),
        ),
      if (defaultTargetPlatform == TargetPlatform.windows)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .detachWindow,
          label: 'Move to New Window',
          leading: Icon(AleraIcons.external, size: 16),
        ),
      if (tab.kind == WorkspaceTabKind.terminal &&
          onOpenExternalTerminal != null)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .openExternalTerminal,
          label: 'Open In External Terminal',
          leading: Icon(AleraIcons.external, size: 16),
        ),
      if (tab.isPreview && _KeepPreviewTabScope.maybeOf(context) != null)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .keepOpen,
          label: 'Keep Open',
          leading: Icon(AleraIcons.pin, size: 16),
        ),
      AleraDropdownEntry<_TabMenuAction>(
        value: .togglePin,
        label: tab.isPinned ? 'Unpin Tab' : 'Pin Tab',
        leading: const Icon(AleraIcons.pin, size: 16),
      ),
      if (tab.kind == WorkspaceTabKind.editor)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .reloadDocument,
          label: 'Reload Document',
          leading: Icon(AleraIcons.refresh, size: 16),
        ),
      if (filePath != null)
        const AleraDropdownEntry<_TabMenuAction>(
          value: .openWithDefaultApplication,
          label: 'Open with Default Application',
          leading: Icon(AleraIcons.external, size: 16),
        ),
      if (filePath != null && workspaceFolderOpener != null)
        AleraDropdownEntry<_TabMenuAction>(
          value: .revealInFileManager,
          label: 'Reveal in ${workspaceFolderOpener.fileManagerLabel}',
          leading: const Icon(AleraIcons.copyFiles, size: 16),
        ),
      if (canCompareWithGit) ...<PopupMenuEntry<_TabMenuAction>>[
        const PopupMenuDivider(height: AleraTokens.space8),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .viewFileHistory,
          label: 'View File History',
          leading: Icon(AleraIcons.gitGraph, size: 16),
        ),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .compareLatestGitRevision,
          label: 'Compare With Latest Git Revision',
          leading: Icon(AleraIcons.diff, size: 16),
        ),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .compareGitBranch,
          label: 'Compare With Branch...',
          leading: Icon(AleraIcons.gitBranch, size: 16),
        ),
      ],
      const AleraDropdownEntry<_TabMenuAction>(
        value: .close,
        label: 'Close',
        leading: Icon(AleraIcons.close, size: 16),
      ),
      AleraDropdownEntry<_TabMenuAction>(
        value: .closeOthers,
        label: 'Close Others',
        leading: Icon(
          AleraIcons.tabUnselected,
          size: 16,
          color: closeOthers.isEmpty
              ? AleraTokens.foregroundFaint
              : AleraTokens.foreground,
        ),
        enabled: closeOthers.isNotEmpty,
      ),
      AleraDropdownEntry<_TabMenuAction>(
        value: .closeRight,
        label: 'Close Tabs to the Right',
        leading: Icon(
          AleraIcons.tab,
          size: 16,
          color: closeRight.isEmpty
              ? AleraTokens.foregroundFaint
              : AleraTokens.foreground,
        ),
        enabled: closeRight.isNotEmpty,
      ),
      ...<PopupMenuEntry<_TabMenuAction>>[
        const PopupMenuDivider(height: AleraTokens.space8),
        const AleraDropdownEntry<_TabMenuAction>(
          value: .changeTitle,
          label: 'Change Title',
          leading: Icon(AleraIcons.edit, size: 16),
        ),
      ],
      if (tab.kind == WorkspaceTabKind.terminal &&
          ref.read(agentTitleAvailableProvider).value == true)
        AleraDropdownEntry<_TabMenuAction>(
          value: .generateTitle,
          label: agentTitleActionLabel(tab.payload),
          enabled: !isAgentTitleGenerating(tab.payload),
          leading: const Icon(AleraIcons.ai, size: 16),
        ),
    ],
  );
  if (selected == null || !context.mounted) {
    return;
  }
  switch (selected) {
    case _TabMenuAction.splitUp:
      onSplit!(.up);
    case _TabMenuAction.splitDown:
      onSplit!(.down);
    case _TabMenuAction.splitLeft:
      onSplit!(.left);
    case _TabMenuAction.splitRight:
      onSplit!(.right);
    case _TabMenuAction.moveToMainWindow:
      if (!await detachedActions!.onMoveToMainWindow()) {
        AleraToast.publish(
          message: 'Could not move the tab back to the main window.',
          tone: AleraToastTone.error,
        );
      }
    case _TabMenuAction.keepOpen:
      _KeepPreviewTabScope.maybeOf(context)?.call(tab.id);
    case _TabMenuAction.togglePin:
      await ref
          .read(workbenchControllerProvider.notifier)
          .setWorkspaceTabPinned(tabId: tab.id, pinned: !tab.isPinned);
    case _TabMenuAction.reloadDocument:
      await reloadWorkspaceEditorDocument(context, ref, tab);
    case _TabMenuAction.openWithDefaultApplication:
      await _openWorkspaceTabFileWithDefaultApplication(
        context,
        ref,
        workspace: workspace,
        tab: tab,
      );
    case _TabMenuAction.revealInFileManager:
      await _revealWorkspaceTabFile(
        context,
        ref,
        workspace: workspace,
        tab: tab,
      );
    case _TabMenuAction.viewFileHistory:
      if (filePath != null) {
        await openWorkspaceFileHistory(
          context: context,
          ref: ref,
          workspace: workspace,
          relativePath: filePath,
        );
      }
    case _TabMenuAction.compareLatestGitRevision:
      if (filePath != null) {
        await compareWorkspaceFileWithLatestGitRevision(
          context: context,
          ref: ref,
          workspace: workspace,
          relativePath: filePath,
          sourceControlScope: sourceControlScope,
        );
      }
    case _TabMenuAction.compareGitBranch:
      if (filePath != null) {
        await compareWorkspaceFileWithBranch(
          context: context,
          ref: ref,
          workspace: workspace,
          relativePath: filePath,
          sourceControlScope: sourceControlScope,
        );
      }
    case _TabMenuAction.close:
      onClose();
    case _TabMenuAction.closeOthers:
      onCloseTabs(closeOthers);
    case _TabMenuAction.closeRight:
      onCloseTabs(closeRight);
    case _TabMenuAction.generateTitle:
      try {
        await ref.read(agentTitleServiceProvider).generate(tab);
      } on Object catch (error) {
        AleraToast.publish(
          message: 'Could not generate title: $error',
          tone: .error,
        );
      }
    case _TabMenuAction.changeTitle:
      final title = await showRenameDialog(
        context,
        title: 'Change Terminal Title',
        labelText: 'Terminal Title',
        initialValue: terminalSession?.displayTitle ?? _workspaceTabTitle(tab),
        confirmLabel: 'Change Title',
      );
      if (title != null) {
        onRename(title);
      }
    case _TabMenuAction.openExternalTerminal:
      await onOpenExternalTerminal?.call(tab);
    case _TabMenuAction.detachWindow:
      final detached = detachedActions;
      if (detached != null) {
        if (!await detached.onMoveToNewWindow()) {
          AleraToast.publish(
            message: 'Could not open detached window.',
            tone: AleraToastTone.error,
          );
        }
        return;
      }
      final descriptor = DetachedTabDescriptor(workspace: workspace, tab: tab);
      final sharesSession = detachedTabWindows.willShareSession(descriptor);
      if (!sharesSession &&
          ref.read(editorSessionRegistryProvider).isDirty(tab.id)) {
        AleraToast.publish(
          message: 'Save this file before moving it to a new window.',
          tone: AleraToastTone.error,
        );
        return;
      }
      if (tab.isPreview) {
        _KeepPreviewTabScope.maybeOf(context)?.call(tab.id);
      }
      try {
        final created = await detachedTabWindows.detach(descriptor);
        if (!created) {
          AleraToast.publish(
            message: 'Could not open detached window.',
            tone: AleraToastTone.error,
          );
        } else if (!sharesSession && tab.kind == WorkspaceTabKind.terminal) {
          ref.read(terminalRuntimeProvider).releaseTab(tab.id);
        }
      } on Object catch (error) {
        AleraToast.publish(
          message: 'Could not open detached window: $error',
          tone: AleraToastTone.error,
        );
      }
  }
}

Future<void> _openWorkspaceTabFileWithDefaultApplication(
  BuildContext context,
  WidgetRef ref, {
  required Workspace workspace,
  required WorkspaceTabRecord tab,
}) async {
  final filePath = tab.filePath;
  if (filePath == null) {
    return;
  }
  final result = await ref
      .read(workspaceFolderOpenerProvider)
      .openWithDefaultApplication(
        terminalAbsolutePath(rootPath: workspace.path, relativePath: filePath),
      );
  if (!context.mounted || result.ok) {
    return;
  }
  AleraToast.show(
    context,
    message:
        result.message ?? 'Could not open item with the default application.',
    tone: .error,
  );
}

Future<void> _revealWorkspaceTabFile(
  BuildContext context,
  WidgetRef ref, {
  required Workspace workspace,
  required WorkspaceTabRecord tab,
}) async {
  final filePath = tab.filePath;
  if (filePath == null) {
    return;
  }
  final result = await ref
      .read(workspaceFolderOpenerProvider)
      .reveal(
        terminalAbsolutePath(rootPath: workspace.path, relativePath: filePath),
      );
  if (!context.mounted || result.ok) {
    return;
  }
  AleraToast.show(
    context,
    message: result.message ?? 'Could not reveal item in file manager.',
    tone: .error,
  );
}
