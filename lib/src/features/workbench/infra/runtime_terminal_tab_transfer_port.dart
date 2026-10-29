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
    implements WorkbenchTerminalTabTransferPort {
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
    final payload = await _client.runtimeRequest(
      'tab.transfer',
      plan.toRuntimePayload(),
    );
    if (payload is! Map) {
      throw StateError('Runtime tab.transfer returned invalid data.');
    }
    return WorkspaceTabRecord.fromJson(Map<String, Object?>.from(payload));
  }
}
