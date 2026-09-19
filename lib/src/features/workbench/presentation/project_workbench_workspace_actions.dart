part of 'project_workbench_sidebar.dart';

/// Context-menu action ids, entry builder, and OS-level workspace action
/// methods extracted from [ProjectWorkbenchSidebar] and [_WorkspaceRow] to keep
/// those presentation files under the line budget. Everything here is
/// library-private and consumed only by the sidebar parts.

const String _renameAction = 'rename';
const String _openProjectSettingsAction = 'open-project-settings';
const String _openFolderAction = 'open-folder';
const String _openExternalEditorAction = 'open-external-editor';
const String _copyPathAction = 'copy-path';
const String _openInBrowserAction = 'open-in-browser';
const String _sleepAction = 'sleep';
const String _archiveAction = 'archive';
const String _restoreAction = 'restore';
const String _manageTagsAction = 'manage-tags';
const String _togglePinAction = 'toggle-pin';
const String _pinWorkspaceTreeAction = 'pin-workspace-tree';
const String _unpinWorkspaceTreeAction = 'unpin-workspace-tree';
const String _setParentAction = 'set-parent';
const String _clearParentAction = 'clear-parent';
const String _setSectionAction = 'set-section';
const String _clearSectionAction = 'clear-section';
const String _removeAction = 'remove';

/// Builds the right-click menu entries for a workspace row. [hasClearParent]
/// gates the "Clear Parent Workspace" item and [canRemove] disables the remove
/// action for the main (non-deletable) workspace. [isArchived] swaps Sleep for
/// Restore; [canRemove] doubles as the "not the main workspace" gate for
/// Archive since main workspaces cannot be archived or removed.
List<PopupMenuEntry<String>> workspaceContextMenuEntries({
  required String fileManagerLabel,
  bool supportsSections = false,
  bool hasSection = false,
  required bool hasClearParent,
  required bool canRemove,
  required bool isPinned,
  required bool isArchived,
  bool hasDescendants = false,
  ExternalEditorSpec? externalEditor,
  List<ExternalEditorSpec> installedExternalEditors =
      const <ExternalEditorSpec>[],
  ValueChanged<ExternalEditorKind>? onExternalEditorPicked,
}) {
  return <PopupMenuEntry<String>>[
    const AleraDropdownEntry<String>(
      value: _renameAction,
      leading: Icon(AleraIcons.edit, size: 16),
      label: 'Rename',
    ),
    AleraDropdownEntry<String>(
      value: _togglePinAction,
      leading: Icon(isPinned ? AleraIcons.pinOff : AleraIcons.pin, size: 16),
      label: isPinned ? 'Unpin Workspace' : 'Pin Workspace',
    ),
    if (hasDescendants)
      const AleraDropdownEntry<String>(
        value: _pinWorkspaceTreeAction,
        leading: Icon(AleraIcons.pin, size: 16),
        label: 'Pin Workspace Tree',
      ),
    if (hasDescendants)
      const AleraDropdownEntry<String>(
        value: _unpinWorkspaceTreeAction,
        leading: Icon(AleraIcons.pinOff, size: 16),
        label: 'Unpin Workspace Tree',
      ),
    const AleraDropdownEntry<String>(
      value: _manageTagsAction,
      leading: Icon(AleraIcons.tag, size: 16),
      label: 'Manage Tags',
    ),
    const AleraDropdownEntry<String>(
      value: _setParentAction,
      leading: Icon(AleraIcons.link, size: 16),
      label: 'Set Parent Workspace',
    ),
    if (hasClearParent)
      const AleraDropdownEntry<String>(
        value: _clearParentAction,
        leading: Icon(AleraIcons.close, size: 16),
        label: 'Clear Parent Workspace',
      ),
    if (supportsSections)
      const AleraDropdownEntry<String>(
        value: _setSectionAction,
        leading: Icon(AleraIcons.folder, size: 16),
        label: 'Set Section',
      ),
    if (supportsSections && hasSection)
      const AleraDropdownEntry<String>(
        value: _clearSectionAction,
        leading: Icon(AleraIcons.folderOff, size: 16),
        label: 'Clear Section',
      ),
    const PopupMenuDivider(height: AleraTokens.space8),
    const AleraDropdownEntry<String>(
      value: _openInBrowserAction,
      leading: Icon(
        AleraIcons.external,
        size: 16,
        color: AleraTokens.foreground,
      ),
      label: 'Open in Browser',
    ),
    AleraDropdownEntry<String>(
      value: _openFolderAction,
      leading: const Icon(
        AleraIcons.folderOpen,
        size: 16,
        color: AleraTokens.foreground,
      ),
      label: 'Open in $fileManagerLabel',
    ),
    if (externalEditor case final editor?)
      AleraDropdownSubmenuEntry<String, ExternalEditorKind>(
        primaryValue: _openExternalEditorAction,
        leading: const Icon(AleraIcons.external, size: 16),
        label: 'Open in ${editor.shortName}',
        childResult: (_) => _openExternalEditorAction,
        onChildResult: onExternalEditorPicked,
        children: externalEditorMenuChildren(
          resolved: editor,
          installed: installedExternalEditors,
        ),
      ),
    const AleraDropdownEntry<String>(
      value: _openProjectSettingsAction,
      leading: Icon(AleraIcons.settings, size: 16),
      label: 'Open in Project Settings',
    ),
    const AleraDropdownEntry<String>(
      value: _copyPathAction,
      leading: Icon(AleraIcons.copy, size: 16, color: AleraTokens.foreground),
      label: 'Copy Path',
    ),
    const PopupMenuDivider(height: AleraTokens.space8),
    if (!isArchived)
      const AleraDropdownEntry<String>(
        value: _sleepAction,
        leading: Icon(
          AleraIcons.theme,
          size: 16,
          color: AleraTokens.foreground,
        ),
        label: 'Sleep',
      ),
    if (isArchived)
      const AleraDropdownEntry<String>(
        value: _restoreAction,
        leading: Icon(
          AleraIcons.unarchive,
          size: 16,
          color: AleraTokens.foreground,
        ),
        label: 'Restore',
      )
    else if (canRemove)
      const AleraDropdownEntry<String>(
        value: _archiveAction,
        leading: Icon(
          AleraIcons.archive,
          size: 16,
          color: AleraTokens.foreground,
        ),
        label: 'Archive',
      ),
    AleraDropdownEntry<String>(
      value: _removeAction,
      leading: Icon(
        AleraIcons.delete,
        size: 16,
        color: canRemove ? AleraTokens.foreground : AleraTokens.foregroundFaint,
      ),
      label: 'Remove',
      enabled: canRemove,
    ),
  ];
}

/// OS-level workspace actions (open folder/browser, copy path, sleep) mixed into
/// the sidebar state so they share its [ref], [context], and [mounted] guard
/// without carrying a [BuildContext] across async gaps.
mixin _WorkspaceSidebarActions on ConsumerState<ProjectWorkbenchSidebar> {
  Future<void> openWorkspaceExternally(
    Workspace workspace,
    ExternalEditorKind kind,
  ) async {
    final result = await ref
        .read(externalEditorLauncherForProvider(kind))
        .openWorkspace(workspace.path);
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message:
            result.message ??
            'Could not open workspace in ${externalEditorSpecs[kind]?.displayName ?? 'the external editor'}.',
        tone: .error,
      );
    }
  }

  Future<void> openWorkspaceFolder(Workspace workspace) async {
    final result = await ref
        .read(workspaceFolderOpenerProvider)
        .open(workspace.path);
    if (!result.ok && mounted) {
      AleraToast.show(
        context,
        message: result.message ?? 'Could not open workspace folder.',
        tone: .error,
      );
    }
  }

  Future<void> copyWorkspacePath(Workspace workspace) async {
    await Clipboard.setData(
      ClipboardData(text: userVisibleWorkspacePath(workspace.path)),
    );
    if (!mounted) {
      return;
    }
    AleraToast.show(context, message: 'Workspace path copied', tone: .success);
  }

  /// Opens the workspace's repository home page in the system browser. The
  /// project-level hosting override is best-effort: a timeout or error falls
  /// back to auto-detection instead of blocking or breaking the click.
  Future<void> openWorkspaceInBrowser(Workspace workspace) async {
    GitHostingProvider? override;
    try {
      override = await ref
          .read(
            effectiveHostingProviderOverrideProvider(workspace.projectId)
                .future,
          )
          .timeout(const Duration(seconds: 2));
    } catch (_) {
      override = null;
    }

    final OpenRepositoryOutcome outcome;
    try {
      outcome = await ref
          .read(repositoryBrowserOpenerProvider)
          .open(repoPath: workspace.path, override: override);
    } catch (error) {
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: 'Could not open the repository: $error',
        tone: .error,
      );
      return;
    }
    if (!mounted) {
      return;
    }
    switch (outcome) {
      case OpenRepositoryOutcome.opened:
        break;
      case OpenRepositoryOutcome.noRemote:
        AleraToast.show(
          context,
          message: 'No git remote configured for this workspace.',
          tone: .info,
        );
      case OpenRepositoryOutcome.undetectable:
        AleraToast.show(
          context,
          message:
              'Could not detect a supported git hosting provider '
              '(GitHub or Azure DevOps).',
          tone: .info,
        );
      case OpenRepositoryOutcome.openFailed:
        AleraToast.show(
          context,
          message: 'Could not open the browser.',
          tone: .error,
        );
    }
  }

  Future<void> sleepWorkspace(Workspace workspace) async {
    final state = ref.read(workbenchControllerProvider);
    final tabs = state.tabsFor(workspace.id);
    final editorRegistry = ref.read(editorSessionRegistryProvider);
    final dirtyEditorCount = tabs
        .where((tab) => editorRegistry.isDirty(tab.id))
        .length;
    final dirtyWarning = dirtyEditorCount == 0
        ? ''
        : dirtyEditorCount == 1
        ? ' One editor has unsaved changes that will be discarded.'
        : ' $dirtyEditorCount editors have unsaved changes that will be discarded.';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AleraConfirmDialog(
        title: 'Sleep Workspace?',
        message:
            'This closes all tabs and terminal sessions for "${workspace.name}". '
            'The workspace, branch, and files will be preserved.$dirtyWarning',
        confirmLabel: 'Sleep',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }

    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .sleepWorkspace(workspace);
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: 'Workspace slept', tone: .success);
    } catch (error) {
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: 'Could not sleep workspace: $error',
        tone: .error,
      );
    }
  }

  Future<void> sleepProject(Project project) async {
    final state = ref.read(workbenchControllerProvider);
    final workspaces = state.workspacesFor(project.id);
    if (workspaces.isEmpty) {
      return;
    }

    final editorRegistry = ref.read(editorSessionRegistryProvider);
    final dirtyEditorCount = workspaces
        .expand((workspace) => state.tabsFor(workspace.id))
        .where((tab) => editorRegistry.isDirty(tab.id))
        .length;
    final dirtyWarning = dirtyEditorCount == 0
        ? ''
        : dirtyEditorCount == 1
        ? ' One editor has unsaved changes that will be discarded.'
        : ' $dirtyEditorCount editors have unsaved changes that will be discarded.';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AleraConfirmDialog(
        title: 'Sleep Project?',
        message:
            'This closes all tabs and terminal sessions for all ${workspaces.length} '
            'workspaces in "${project.name}". Worktrees, branches, and files will '
            'be preserved.$dirtyWarning',
        confirmLabel: 'Sleep All',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }

    try {
      final controller = ref.read(workbenchControllerProvider.notifier);
      for (final workspace in workspaces) {
        await controller.sleepWorkspace(workspace);
      }
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: context.tr('Project workspaces slept'),
        tone: .success,
      );
    } catch (error) {
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: context.tr('Could not sleep project: $error'),
        tone: .error,
      );
    }
  }

  Future<void> archiveWorkspace(Workspace workspace) async {
    if (workspace.isMain || workspace.isArchived) {
      return;
    }
    final state = ref.read(workbenchControllerProvider);
    final tabs = state.tabsFor(workspace.id);
    final editorRegistry = ref.read(editorSessionRegistryProvider);
    final dirtyEditorCount = tabs
        .where((tab) => editorRegistry.isDirty(tab.id))
        .length;
    final dirtyWarning = dirtyEditorCount == 0
        ? ''
        : dirtyEditorCount == 1
        ? ' One editor has unsaved changes that will be discarded.'
        : ' $dirtyEditorCount editors have unsaved changes that will be discarded.';
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AleraConfirmDialog(
        title: 'Archive Workspace?',
        message:
            'This closes all tabs and terminal sessions for "${workspace.name}" '
            'and moves it to the Archived section. The workspace, branch, and '
            'files will be preserved.$dirtyWarning',
        confirmLabel: 'Archive',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) {
      return;
    }

    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .archiveWorkspace(workspace);
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: 'Workspace archived', tone: .success);
    } catch (error) {
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: 'Could not archive workspace: $error',
        tone: .error,
      );
    }
  }

  /// Restores the workspace and opens it - tapping an archived row and the
  /// Restore menu item share this entry point.
  Future<void> restoreWorkspace(Workspace workspace) async {
    if (!workspace.isArchived) {
      return;
    }
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .restoreWorkspace(workspace, select: true);
    } catch (error) {
      if (!mounted) {
        return;
      }
      AleraToast.show(
        context,
        message: 'Could not restore workspace: $error',
        tone: .error,
      );
    }
  }
}
