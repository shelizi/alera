import 'package:alera/src/app/providers.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Confirms before closing tabs if any editor has unsaved changes or if any
/// terminal tab has active child processes or running agents.
Future<bool> confirmCloseWorkspaceTabs(
  BuildContext context,
  WidgetRef ref,
  List<WorkspaceTabRecord> tabs,
  List<String> tabIds,
) async {
  final registry = ref.read(editorSessionRegistryProvider);
  final dirty = <String>[
    for (final tab in tabs)
      if (tabIds.contains(tab.id) && registry.isDirty(tab.id)) tab.title,
  ];
  if (dirty.isNotEmpty) {
    if (!context.mounted) return false;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AleraConfirmDialog(
        title: dirty.length == 1
            ? 'Close Unsaved Editor?'
            : 'Close Unsaved Editors?',
        message: dirty.length == 1
            ? '${dirty.first} has unsaved changes.'
            : '${dirty.length} editor tabs have unsaved changes.',
        confirmLabel: 'Close',
        destructive: true,
      ),
    );
    if (confirmed != true) return false;
  }
  if (!context.mounted) return false;

  final settings = ref.read(settingsControllerProvider).terminal;
  if (!settings.confirmCloseRunningProcesses) {
    return true;
  }

  final targetTerminals = <WorkspaceTabRecord>[
    for (final tab in tabs)
      if (tabIds.contains(tab.id) && tab.kind == WorkspaceTabKind.terminal) tab,
  ];
  if (targetTerminals.isEmpty) {
    return true;
  }

  final agentStatuses = ref.read(agentStatusControllerProvider);
  final client = ref.read(terminalHostClientProvider);

  final busyTabs = <WorkspaceTabRecord>[];
  final activeAgentTitles = <String>[];
  final runningProcessNames = <String>[];

  for (final tab in targetTerminals) {
    final sessionId = tab.terminalSessionId;
    final agent = agentStatuses[sessionId];
    if (agent != null && agent.state == AgentStatusState.working) {
      busyTabs.add(tab);
      activeAgentTitles.add(tab.title);
      continue;
    }
    try {
      final processes = await client.getTerminalRunningProcesses(sessionId);
      if (processes.isNotEmpty) {
        busyTabs.add(tab);
        runningProcessNames.addAll(processes);
      }
    } catch (_) {
      // Host unreachable or timed out; do not block close.
    }
  }

  if (!context.mounted || busyTabs.isEmpty) {
    return true;
  }

  final String title;
  final String message;
  if (busyTabs.length == 1) {
    final busyTab = busyTabs.first;
    if (activeAgentTitles.isNotEmpty) {
      title = 'Stop Running Agent?';
      message =
          'An agent is actively working in "${busyTab.title}". Closing will terminate the session.';
    } else {
      title = 'Stop Running Command?';
      final names = runningProcessNames.toSet().join(', ');
      message = names.isNotEmpty
          ? 'The process "$names" is still running in "${busyTab.title}". Closing will terminate it.'
          : 'A command is still running in "${busyTab.title}". Closing will terminate it.';
    }
  } else {
    title = 'Close Busy Terminals?';
    message =
        '${busyTabs.length} terminal tabs have running processes or active agents. Closing will terminate them.';
  }

  final confirmed = await showDialog<bool>(
    context: context,
    builder: (context) => AleraConfirmDialog(
      title: title,
      message: message,
      confirmLabel: 'Stop And Close',
      destructive: true,
    ),
  );
  return confirmed == true;
}
