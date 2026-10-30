import 'dart:async';

import 'package:alera_mobile/src/features/runtime/domain/runtime_client_surfaces.dart';
import 'package:alera_mobile/src/features/runtime/domain/workspace_tab_summary.dart';
import 'package:alera_mobile/src/features/terminal/application/terminal_session_controller.dart';
import 'package:alera_mobile/src/features/terminal/application/terminal_providers.dart';
import 'package:alera_mobile/src/features/workbench/application/workbench_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'tabs_controller.g.dart';

/// Tabs of one workspace. The mobile app shows one tab at a time; splits stay
/// a desktop concept.
@riverpod
class TabsController extends _$TabsController {
  final Map<String, String> _runtimeTitles = <String, String>{};

  @override
  Future<List<WorkspaceTabSummary>> build(
    String hostId,
    String workspaceId,
  ) async {
    final client = await ref.watch(terminalClientProvider(hostId).future);
    final subscription = client.events.listen((event) {
      if (event.name == 'workspaceTabsChanged') {
        ref.invalidateSelf();
      } else if (client.supportsTerminalTitles &&
          event.name == 'terminalTitleChanged') {
        _applyRuntimeTitleEvent(event);
      }
    });
    ref.onDispose(subscription.cancel);
    final tabs = await client.listTabs(workspaceId);
    if (!client.supportsTerminalTitles) {
      _runtimeTitles.clear();
      return tabs;
    }
    final tabIds = <String>{for (final tab in tabs) tab.id};
    _runtimeTitles.removeWhere((tabId, _) => !tabIds.contains(tabId));
    for (final tab in tabs) {
      final runtimeTitle = tab.runtimeTitle;
      if (runtimeTitle != null) {
        _runtimeTitles[tab.id] = runtimeTitle;
      }
    }
    return <WorkspaceTabSummary>[
      for (final tab in tabs)
        if (_runtimeTitles[tab.id] case final String runtimeTitle)
          tab.copyWithRuntimeTitle(runtimeTitle)
        else
          tab,
    ];
  }

  void _applyRuntimeTitleEvent(MobileRuntimeEvent event) {
    final eventWorkspaceId = event.payload['workspaceId'];
    final tabId = event.payload['tabId'];
    final title = event.payload['title'];
    if (eventWorkspaceId != workspaceId ||
        tabId is! String ||
        title is! String) {
      return;
    }
    _runtimeTitles[tabId] = title;
    final tabs = state.value;
    if (tabs == null || !tabs.any((tab) => tab.id == tabId)) {
      return;
    }
    state = AsyncData(<WorkspaceTabSummary>[
      for (final tab in tabs)
        if (tab.id == tabId) tab.copyWithRuntimeTitle(title) else tab,
    ]);
  }

  /// Creates a terminal tab titled after the next free "Terminal N" slot and
  /// returns its tab id.
  Future<String> createTerminalTab() async {
    final client = await ref.read(terminalClientProvider(hostId).future);
    final existing = await future;
    final terminalCount = existing.where((tab) => tab.isTerminal).length;
    final session = await client.createTerminal(
      workspaceId,
      title: 'Terminal ${terminalCount + 1}',
    );
    // The attach that follows owns the session; creating must not keep this
    // controller attached.
    await client.detachTerminal(session.attachment.sessionId);
    ref.invalidateSelf();
    return session.tab.id;
  }

  Future<bool> closeTab(WorkspaceTabSummary tab) async {
    if (tab.hasMalformedTransferEvidence) {
      throw StateError(
        'The Terminal transfer receipt is malformed. Preserve this Tab and inspect its original Host record before closing.',
      );
    }
    if (tab.isTerminal && tab.hasTransferEvidence) {
      // Capture the *original, already rendered* phone view before any await
      // or Host lookup. A background tab must first be opened and attached;
      // never manufacture an attachment solely to authorize its deletion.
      final provider = terminalSessionControllerProvider(hostId, tab.id);
      if (!ref.exists(provider)) {
        throw StateError(
          'Open the moved Terminal on this phone before closing it.',
        );
      }
      final originalView = ref.read(provider.notifier);
      final observedAttachment = originalView.observedAttachedView;
      if (observedAttachment == null) {
        throw StateError(
          'The moved Terminal is not attached to a verified phone view.',
        );
      }
      final workspaceClient = await ref.read(
        workspaceClientProvider(hostId).future,
      );
      if (!ref.mounted) return false;
      if (!ref.exists(provider) ||
          !identical(ref.read(provider.notifier), originalView) ||
          originalView.observedAttachedView != observedAttachment) {
        // The same logical View ID may have moved to a different Socket
        // while resolving the Workspace client. That new attachment is
        // not the one that initiated this Tab Close.
        throw StateError(
          'Terminal view reattached while preparing Close. Retry from the current visible view.',
        );
      }
      if (workspaceClient is! MobileObservedViewLeaseTabRemoval) {
        throw StateError(
          'This mobile runtime cannot safely close a moved Terminal.',
        );
      }
      // Unlike a never-moved tab, DO NOT call terminal.terminate first:
      // Host tab.remove performs the receipt/PTY/last-view check and closes
      // the process in one guarded serialized operation.
      await (workspaceClient as MobileObservedViewLeaseTabRemoval)
          .removeObservedTabWithViewLease(tab, observedAttachment.viewLeaseId);
      if (ref.mounted) ref.invalidateSelf();
      return true;
    }
    final client = await ref.read(terminalClientProvider(hostId).future);
    if (!ref.mounted) {
      return false;
    }
    if (tab.isTerminal) {
      try {
        if (client is MobileObservedTerminalTermination) {
          await (client as MobileObservedTerminalTermination)
              .terminateObservedTerminal(tab);
        } else if (tab.hasTransferEvidence) {
          throw StateError(
            'This Terminal moved to another workspace. Update the mobile client before closing it.',
          );
        } else {
          await client.terminateSession(tab.terminalSessionId);
        }
      } on Object {
        // Legacy never-moved tabs may already have an exited PTY; deleting
        // their durable tab still makes sense. A moved Terminal is different:
        // rejection can mean this window owns an obsolete transfer receipt.
        if (tab.hasTransferEvidence) rethrow;
      }
    }
    if (!ref.mounted) {
      return false;
    }
    final workspaceClient = await ref.read(
      workspaceClientProvider(hostId).future,
    );
    if (workspaceClient is MobileObservedTabRemoval) {
      await (workspaceClient as MobileObservedTabRemoval).removeObservedTab(
        tab,
      );
    } else if (tab.hasTransferEvidence) {
      throw StateError(
        'This Terminal changed workspaces. Refresh before deleting it.',
      );
    } else {
      await workspaceClient.removeTab(tab.id);
    }
    if (ref.mounted) {
      ref.invalidateSelf();
    }
    return true;
  }

  Future<void> generateTitle(WorkspaceTabSummary tab) async {
    final client = await ref.read(workspaceClientProvider(hostId).future);
    if (client is! MobileAgentTitleClient) {
      throw UnsupportedError('This runtime cannot generate agent titles.');
    }
    await (client as MobileAgentTitleClient).generateAgentTitle(tab);
    if (ref.mounted) ref.invalidateSelf();
  }

  Future<void> renameTab(WorkspaceTabSummary tab, String title) async {
    final client = await ref.read(workspaceClientProvider(hostId).future);
    await client.renameTab(tab.id, title);
    ref.invalidateSelf();
  }
}
