import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/keyboard/presentation/keyboard_command_palette_dialog.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/workbench_close_confirmation.dart';
import 'package:alera/src/features/workbench/presentation/workbench_dialog_launchers.dart';
import 'package:alera/src/features/workbench/presentation/workbench_editor_reload.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Maps a [KeyboardActionId] to concrete app behavior, reusing existing
/// controller methods and dialog flows. Construct one per dispatch with the
/// current [ref] and a [context] suitable for showing dialogs/toasts.
class const KeyboardCommandDispatcher({
  required final WidgetRef ref,
  required final BuildContext context,
  final TerminalSessionHandle? terminalSession,
}) {
  void dispatch(KeyboardActionId id) {
    switch (id) {
      case KeyboardActionId.openSettings:
        unawaited(openSettingsDialog(context));
      case KeyboardActionId.openAutomations:
        unawaited(openAutomationsDialog(context));
      case KeyboardActionId.openQuickOpen:
        unawaited(showQuickOpenFlow(context, ref));
      case KeyboardActionId.openCommandPalette:
        unawaited(
          showKeyboardCommandPalette(
            context,
            onExecute: (command) => dispatch(command),
          ),
        );
      case KeyboardActionId.addProject:
        unawaited(showAddProjectFlow(context, ref));
      case KeyboardActionId.toggleSidebar:
        _toggleSidebar();
      case KeyboardActionId.createWorkspace:
        final project = ref.read(workbenchControllerProvider).activeProject;
        unawaited(
          showCreateWorkspaceFlow(context, ref, initialProject: project),
        );
      case KeyboardActionId.openWorkspaceInZed:
        _openWorkspaceExternally();
      case KeyboardActionId.navigateBack:
        unawaited(ref.read(workbenchControllerProvider.notifier).goBack());
      case KeyboardActionId.navigateForward:
        unawaited(ref.read(workbenchControllerProvider.notifier).goForward());
      case KeyboardActionId.findInFiles:
        _showContextPanel(.search);
      case KeyboardActionId.findInTerminal:
        _openTerminalSearch();
      case KeyboardActionId.toggleTerminalComposer:
        _toggleTerminalComposer();
      case KeyboardActionId.replaceInFiles:
        _showContextPanel(.search);
      case KeyboardActionId.saveFile:
        _saveActiveEditor();
      case KeyboardActionId.goToDefinition:
        _runActiveEditorNavigation(.goToDefinition);
      case KeyboardActionId.goToImplementation:
        _runActiveEditorNavigation(.goToImplementation);
      case KeyboardActionId.findReferences:
        _runActiveEditorNavigation(.findReferences);
      case KeyboardActionId.reloadDocument:
        _reloadActiveEditor();
      case KeyboardActionId.newTerminalTab:
        _newTerminalTab();
      case KeyboardActionId.closeTab:
        _closeActiveTab();
      case KeyboardActionId.nextTab:
        _cycleTab(1);
      case KeyboardActionId.previousTab:
        _cycleTab(-1);
      case KeyboardActionId.goToTab1:
      case KeyboardActionId.goToTab2:
      case KeyboardActionId.goToTab3:
      case KeyboardActionId.goToTab4:
      case KeyboardActionId.goToTab5:
      case KeyboardActionId.goToTab6:
      case KeyboardActionId.goToTab7:
      case KeyboardActionId.goToTab8:
        _goToTabIndex(id.tabIndex! - 1);
      case KeyboardActionId.goToTab9:
        _goToLastTab();
      case KeyboardActionId.splitRight:
        _split(.right);
      case KeyboardActionId.splitDown:
        _split(.down);
      case KeyboardActionId.closeSplit:
        _closeSplit();
    }
  }

  void _openWorkspaceExternally() {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null) return;
    unawaited(() async {
      final spec = await ref.read(resolvedExternalEditorProvider.future);
      if (spec == null) {
        if (context.mounted) {
          AleraToast.show(
            context,
            message: 'No external editor is installed.',
            tone: .error,
          );
        }
        return;
      }
      final result = await ref
          .read(externalEditorLauncherForProvider(spec.kind))
          .openWorkspace(workspace.path);
      if (!result.ok && context.mounted) {
        AleraToast.show(
          context,
          message:
              result.message ??
              'Could not open workspace in ${spec.displayName}.',
          tone: .error,
        );
      }
    }());
  }

  void _toggleSidebar() {
    final controller = ref.read(workbenchControllerProvider.notifier);
    final collapsed = ref.read(workbenchControllerProvider).collapsed;
    controller.setCollapsed(!collapsed);
  }

  void _newTerminalTab() {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null) {
      return;
    }
    final controller = ref.read(workbenchControllerProvider.notifier);
    final runtime = ref.read(terminalRuntimeProvider);
    unawaited(() async {
      final tab = await controller.createTerminalTab(workspace);
      runtime.sessionFor(workspace: workspace, tab: tab).requestFocus();
    }());
  }

  void _showContextPanel(WorkbenchContextPanelTab tab) {
    final state = ref.read(workbenchControllerProvider);
    if (state.activeWorkspace == null) {
      return;
    }
    final controller = ref.read(workbenchControllerProvider.notifier);
    controller.setContextPanelTab(tab);
    controller.setRightSidebarVisible(true);
  }

  void _openTerminalSearch() {
    final directSession = terminalSession;
    if (directSession != null) {
      directSession.openSearch();
      return;
    }
    final state = ref.read(workbenchControllerProvider);
    final tab = state.activeWorkspaceTab;
    if (tab == null || tab.kind != WorkspaceTabKind.terminal) {
      return;
    }
    ref.read(terminalRuntimeProvider).peekSession(tab.id)?.openSearch();
  }

  void _toggleTerminalComposer() {
    final directSession = terminalSession;
    if (directSession != null) {
      directSession.composerController.toggle();
      return;
    }
    final tab = ref.read(workbenchControllerProvider).activeWorkspaceTab;
    if (tab == null || tab.kind != WorkspaceTabKind.terminal) {
      return;
    }
    ref
        .read(terminalRuntimeProvider)
        .peekSession(tab.id)
        ?.composerController
        .toggle();
  }

  void _saveActiveEditor() {
    final tab = ref.read(workbenchControllerProvider).activeWorkspaceTab;
    if (tab == null || tab.kind != WorkspaceTabKind.editor) {
      return;
    }
    unawaited(ref.read(editorSessionRegistryProvider).save(tab.id));
  }

  void _runActiveEditorNavigation(EditorSessionNavigationCommand command) {
    final tab = ref.read(workbenchControllerProvider).activeWorkspaceTab;
    if (tab == null || tab.kind != WorkspaceTabKind.editor) {
      return;
    }
    unawaited(
      ref
          .read(editorSessionRegistryProvider)
          .runNavigationCommand(tab.id, command),
    );
  }

  void _reloadActiveEditor() {
    final tab = ref.read(workbenchControllerProvider).activeWorkspaceTab;
    if (tab == null || tab.kind != WorkspaceTabKind.editor) {
      return;
    }
    unawaited(reloadWorkspaceEditorDocument(context, ref, tab));
  }

  void _closeActiveTab() {
    final state = ref.read(workbenchControllerProvider);
    final workspace = state.activeWorkspace;
    final tab = state.activeWorkspaceTab;
    if (workspace == null || tab == null) {
      return;
    }
    final tabs = state.tabsFor(workspace.id);
    unawaited(() async {
      final confirmed = await confirmCloseWorkspaceTabs(
        context,
        ref,
        tabs,
        <String>[tab.id],
      );
      if (!confirmed || !context.mounted) {
        return;
      }
      // The controller disposes the terminal handle and editor document.
      await ref
          .read(workbenchControllerProvider.notifier)
          .closeWorkspaceTab(workspace: workspace, tabId: tab.id);
    }());
  }

  void _cycleTab(int delta) {
    final state = ref.read(workbenchControllerProvider);
    final layout = state.activeLayout;
    final workspace = state.activeWorkspace;
    final group = layout?.activeGroup;
    if (layout == null || workspace == null || group == null) {
      return;
    }
    final tabIds = group.tabIds;
    if (tabIds.length < 2) {
      return;
    }
    final current = group.activeTabId;
    final currentIndex = current == null ? 0 : tabIds.indexOf(current);
    final base = currentIndex < 0 ? 0 : currentIndex;
    final nextIndex = (base + delta) % tabIds.length;
    final wrapped = nextIndex < 0 ? nextIndex + tabIds.length : nextIndex;
    ref
        .read(workbenchControllerProvider.notifier)
        .setActiveWorkspaceTab(
          workspaceId: workspace.id,
          groupId: layout.activeGroupId,
          tabId: tabIds[wrapped],
        );
  }

  void _goToTabIndex(int index) {
    final state = ref.read(workbenchControllerProvider);
    final layout = state.activeLayout;
    final workspace = state.activeWorkspace;
    final group = layout?.activeGroup;
    if (layout == null || workspace == null || group == null) {
      return;
    }
    if (index < 0 || index >= group.tabIds.length) {
      return;
    }
    ref
        .read(workbenchControllerProvider.notifier)
        .setActiveWorkspaceTab(
          workspaceId: workspace.id,
          groupId: layout.activeGroupId,
          tabId: group.tabIds[index],
        );
  }

  void _goToLastTab() {
    final group = ref
        .read(workbenchControllerProvider)
        .activeLayout
        ?.activeGroup;
    if (group == null || group.tabIds.isEmpty) {
      return;
    }
    _goToTabIndex(group.tabIds.length - 1);
  }

  void _split(WorkbenchDropZone zone) {
    final state = ref.read(workbenchControllerProvider);
    final workspace = state.activeWorkspace;
    final layout = state.activeLayout;
    if (workspace == null || layout == null) {
      return;
    }
    final controller = ref.read(workbenchControllerProvider.notifier);
    final runtime = ref.read(terminalRuntimeProvider);
    unawaited(() async {
      final tab = await controller.splitWorkbenchGroupWithTerminal(
        workspace: workspace,
        groupId: layout.activeGroupId,
        zone: zone,
      );
      runtime.sessionFor(workspace: workspace, tab: tab).requestFocus();
    }());
  }

  void _closeSplit() {
    final state = ref.read(workbenchControllerProvider);
    final workspace = state.activeWorkspace;
    final layout = state.activeLayout;
    if (workspace == null || layout == null || layout.groups.length < 2) {
      return;
    }
    unawaited(
      ref
          .read(workbenchControllerProvider.notifier)
          .mergeWorkbenchGroupIntoSibling(
            workspaceId: workspace.id,
            groupId: layout.activeGroupId,
          ),
    );
  }
}
