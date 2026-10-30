import 'package:alera_mobile/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera_mobile/src/features/runtime/domain/workspace_sidebar_snapshot.dart';
import 'package:alera_mobile/src/features/runtime/domain/runtime_client_surfaces.dart';
import 'package:alera_mobile/src/features/terminal/application/tabs_controller.dart';
import 'package:alera_mobile/src/features/terminal/application/terminal_providers.dart';
import 'package:alera_mobile/src/features/workbench/application/workbench_providers.dart';
import 'package:alera_mobile/src/features/workbench/application/workspace_list_controller.dart';
import 'package:alera_mobile/src/features/workbench/presentation/agent_identity_icon.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

Future<void> closeWorkspaceAgentTerminal(
  BuildContext context,
  WidgetRef ref,
  AgentPresenceSummary status, {
  required String hostId,
  required String workspaceId,
}) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (dialogContext) => AleraConfirmDialog(
      title: 'Close Agent Terminal',
      message: 'Close the ${agentDisplayName(status.agentType)} terminal?',
      confirmLabel: 'Close',
    ),
  );
  if (confirmed != true || !context.mounted) {
    return;
  }
  try {
    // Close via host clients directly. tabsControllerProvider is autoDispose
    // and usually has no watchers on the workspace list, so routing through
    // it can dispose mid-await and throw on the notifier's Ref.
    final terminalClient = await ref.read(
      terminalClientProvider(hostId).future,
    );
    final observedTabs = await terminalClient.listTabs(workspaceId);
    final matches = observedTabs.where(
      (tab) =>
          tab.id == status.tabId &&
          tab.terminalSessionId == status.terminalSessionId &&
          tab.workspaceId == workspaceId,
    );
    if (matches.length != 1) {
      throw StateError(
        'Terminal changed workspaces or no longer exists. Refresh before closing it.',
      );
    }
    final tab = matches.single;
    try {
      if (terminalClient is MobileObservedTerminalTermination) {
        await (terminalClient as MobileObservedTerminalTermination)
            .terminateObservedTerminal(tab);
      } else if (tab.payload['lastTerminalTabTransfer'] != null) {
        throw StateError(
          'Mobile runtime cannot safely close a moved Terminal.',
        );
      } else {
        await terminalClient.terminateSession(status.terminalSessionId);
      }
    } on Object {
      if (tab.payload['lastTerminalTabTransfer'] != null) rethrow;
    }
    final workspaceClient = await ref.read(
      workspaceClientProvider(hostId).future,
    );
    if (workspaceClient is MobileObservedTabRemoval) {
      await (workspaceClient as MobileObservedTabRemoval).removeObservedTab(
        tab,
      );
    } else if (tab.payload['lastTerminalTabTransfer'] != null) {
      throw StateError('Mobile runtime cannot safely delete a moved Terminal.');
    } else {
      await workspaceClient.removeTab(status.tabId);
    }
    ref.invalidate(workspaceListControllerProvider(hostId));
    ref.invalidate(tabsControllerProvider(hostId, workspaceId));
  } on Object catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Could not close agent terminal: $error')),
      );
    }
  }
}
