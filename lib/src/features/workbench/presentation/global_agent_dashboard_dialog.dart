import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

Future<void> showGlobalAgentDashboard(BuildContext context) {
  return showDialog<void>(
    context: context,
    builder: (_) => const GlobalAgentDashboardDialog(),
  );
}

/// Global scope is deliberate: this watches the full Workbench inventory and
/// current Agent map, not the selected project's subset.
class const GlobalAgentDashboardDialog({super.key})
    extends ConsumerStatefulWidget {
  @override
  ConsumerState<GlobalAgentDashboardDialog> createState() =>
      _GlobalAgentDashboardDialogState();
}

class _GlobalAgentDashboardDialogState
    extends ConsumerState<GlobalAgentDashboardDialog> {
  String _query = '';

  Future<void> _focus(GlobalAgentLiveRow row) async {
    final state = ref.read(workbenchControllerProvider);
    final status = ref.read(
      agentStatusControllerProvider,
    )[row.tab.terminalSessionId];
    final current = state.tabsFor(row.status.workspaceId);
    if (status == null ||
        status.workspaceId != row.status.workspaceId ||
        status.tabId != row.tab.id ||
        status.terminalSessionId != row.tab.terminalSessionId ||
        !current.any(
          (tab) =>
              tab.id == row.tab.id &&
              tab.terminalSessionId == row.tab.terminalSessionId,
        )) {
      return;
    }
    await ref
        .read(workbenchControllerProvider.notifier)
        .selectWorkspaceTab(
          workspaceId: status.workspaceId,
          tabId: status.tabId,
        );
    if (mounted) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final workbench = ref.watch(workbenchControllerProvider);
    final statuses = ref.watch(agentStatusControllerProvider);
    final all = projectGlobalAgentLiveRows(workbench, statuses);
    final rows = all.where((row) => row.matches(_query)).toList();
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return Dialog(
      backgroundColor: AleraTokens.bg,
      child: SizedBox(
        width: 880,
        height: 660,
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      'Global Agent Dashboard',
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                  ),
                  IconButton(
                    tooltip: 'Close',
                    onPressed: () => Navigator.of(context).pop(),
                    icon: const Icon(Icons.close),
                  ),
                ],
              ),
              const SizedBox(height: AleraTokens.space8),
              Text(
                'All registered projects and worktrees. Live Agent status only; '
                'historical and semantic search will be added separately.',
                style: muted,
              ),
              const SizedBox(height: AleraTokens.space16),
              TextField(
                autofocus: true,
                decoration: const InputDecoration(
                  prefixIcon: Icon(Icons.search),
                  labelText: 'Search Live Sessions',
                  hintText: 'Project, Agent, overall objective or progress',
                ),
                onChanged: (value) => setState(() => _query = value),
              ),
              const SizedBox(height: AleraTokens.space12),
              Text(
                '${rows.length} of ${all.length} Agent sessions',
                style: muted,
              ),
              const SizedBox(height: AleraTokens.space12),
              Expanded(
                child: rows.isEmpty
                    ? Center(
                        child: Text(
                          all.isEmpty
                              ? 'No verified live Agent sessions in registered workspaces.'
                              : 'No matching live Agent sessions.',
                          style: muted,
                        ),
                      )
                    : ListView.separated(
                        itemCount: rows.length,
                        separatorBuilder: (_, _) =>
                            const SizedBox(height: AleraTokens.space8),
                        itemBuilder: (context, index) {
                          final row = rows[index];
                          return DecoratedBox(
                            decoration: BoxDecoration(
                              color: AleraTokens.surface,
                              border: Border.all(
                                color: AleraTokens.borderSubtle,
                              ),
                              borderRadius: BorderRadius.circular(
                                AleraTokens.radiusMd,
                              ),
                            ),
                            child: Padding(
                              padding: const EdgeInsets.all(
                                AleraTokens.space16,
                              ),
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Row(
                                    children: [
                                      Expanded(
                                        child: Text(
                                          '${row.project} / ${row.workspace}',
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: Theme.of(context)
                                              .textTheme
                                              .titleSmall,
                                        ),
                                      ),
                                      Text(
                                        '${row.status.agentType.key} - '
                                        '${row.status.state.key}',
                                        style: muted,
                                      ),
                                    ],
                                  ),
                                  const SizedBox(height: AleraTokens.space8),
                                  Text(
                                    row.goal.isEmpty ? row.tab.title : row.goal,
                                    maxLines: 3,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                  if (row.progress.isNotEmpty) ...[
                                    const SizedBox(height: AleraTokens.space4),
                                    Text(
                                      row.progress,
                                      maxLines: 2,
                                      overflow: TextOverflow.ellipsis,
                                      style: muted,
                                    ),
                                  ],
                                  Align(
                                    alignment: Alignment.centerRight,
                                    child: TextButton.icon(
                                      onPressed: () => _focus(row),
                                      icon: const Icon(Icons.open_in_new),
                                      label: const Text('Open Agent'),
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
          ),
        ),
      ),
    );
  }
}
