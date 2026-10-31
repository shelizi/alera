import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_terminal_tab_transfer_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';

/// Production adapter for the future atomic tab.transfer RPC. Instantiate
/// only after the host negotiates/supports full live-session ownership moves;
/// do not expose a UI drop callback when this verb is unavailable.
final class RuntimeTerminalTabTransferPort
    implements
        WorkbenchTerminalTabTransferPort,
        WorkbenchTerminalTabTransferRecoveryPort {
  const RuntimeTerminalTabTransferPort({
    required WorkbenchRepository repository,
    required RuntimeHostClient client,
  }) : _repository = repository,
       _client = client;

  final WorkbenchRepository _repository;
  final RuntimeHostClient _client;

  @override
  Future<List<WorkspaceTabRecord>> listTabs(String workspaceId) =>
      _repository.listWorkspaceTabs(workspaceId);

  @override
  Future<WorkbenchLayout?> findLayout(String workspaceId) =>
      _repository.findWorkbenchLayout(workspaceId);

  @override
  Future<WorkspaceTabRecord> commit(
    WorkbenchTerminalTabTransferPlan plan,
  ) async {
    // Do not fall back to tab.upsert on older sidecars. A missing route must
    // reject the operation rather than orphan a live PTY.
    final Object? payload;
    try {
      payload = await _client.runtimeRequest(
        'tab.transfer',
        plan.toRuntimePayload(),
      );
    } on TerminalHostRequestTimeoutException catch (error) {
      if (error.requestType != 'tab.transfer') rethrow;
      return await _recoverCommittedReply(plan) ??
          (throw TerminalTransferOutcomeUnknownException(
            tabId: plan.tabId,
            clientMutationId: plan.clientMutationId,
          ));
    } on TerminalHostConnectionClosedException {
      // A disconnected transport does not prove the write failed. Query the
      // original operation's receipt after reconnect; never generate a new
      // mutation ID or close and recreate its existing PTY.
      return await _recoverCommittedReply(plan) ??
          (throw TerminalTransferOutcomeUnknownException(
            tabId: plan.tabId,
            clientMutationId: plan.clientMutationId,
          ));
    }
    if (payload is! Map) {
      throw StateError('Runtime tab.transfer returned invalid data.');
    }
    return WorkspaceTabRecord.fromJson(Map<String, Object?>.from(payload));
  }

  Future<WorkspaceTabRecord?> _recoverCommittedReply(
    WorkbenchTerminalTabTransferPlan plan,
  ) async {
    try {
      final status = await _client.runtimeRequest(
        'tab.transfer.status',
        <String, Object?>{
          'tabId': plan.tabId,
          'sessionId': plan.sessionId,
          'sourceWorkspaceId': plan.sourceWorkspaceId,
          'targetWorkspaceId': plan.targetWorkspaceId,
          'clientMutationId': plan.clientMutationId,
        },
      );
      if (status is Map && status['recoveryRequired'] == true) {
        // Host may have lost the row, received a conflicting/erased receipt,
        // or failed to prove the original PTY. None of these proves success,
        // failure, or permission to clear the original Journal.
        throw TerminalTransferOutcomeUnknownException(
          tabId: plan.tabId,
          clientMutationId: plan.clientMutationId,
          hostRecoveryRequired: true,
        );
      }
      if (status is! Map || status['applied'] != true) return null;
      final stored = status['tab'];
      if (stored is! Map) return null;
      final tab = WorkspaceTabRecord.fromJson(
        Map<String, Object?>.from(stored),
      );
      // Even if a future host returns an incomplete/malformed status,
      // verify the tab, Session and exact original receipt before reporting
      // success. Only the host can prove the live PTY side of the move.
      final receipt = tab.payload['lastTerminalTabTransfer'];
      if (tab.kind != WorkspaceTabKind.terminal ||
          tab.id != plan.tabId ||
          tab.workspaceId != plan.targetWorkspaceId ||
          tab.terminalSessionId != plan.sessionId ||
          receipt is! Map ||
          receipt['id'] != plan.clientMutationId ||
          receipt['from'] != plan.sourceWorkspaceId ||
          receipt['to'] != plan.targetWorkspaceId) {
        return null;
      }
      return tab;
    } on TerminalTransferOutcomeUnknownException {
      rethrow;
    } on Object {
      // Connection can remain unavailable after the first timeout. Preserve
      // an *unknown* outcome rather than falsely marking failure or success.
      return null;
    }
  }

  @override
  Future<WorkspaceTabRecord?> confirmCommitted(
    WorkbenchTerminalTabTransferPlan original,
  ) => _recoverCommittedReply(original);
}
