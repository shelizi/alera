import 'dart:convert';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_launch_dialog.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
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
  final TextEditingController _instruction = TextEditingController();
  ({String sessionId, String workspaceId, String tabId})? _target;
  bool _sending = false;
  bool _controlling = false;
  ({String sessionId, String workspaceId, String tabId, bool terminate})?
  _armedControl;
  String? _sendFeedback;

  Future<void> _controlAgent(GlobalAgentLiveRow row, bool terminate) async {
    if (_controlling || _sending) return;
    final identity = _identity(row);
    final intent = (
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
      terminate: terminate,
    );
    if (_armedControl != intent) {
      setState(() {
        _armedControl = intent;
        _sendFeedback = terminate
            ? 'Confirm terminate: this ends the Host PTY and may remove its '
                  'retained Terminal history. Hide card instead to keep working.'
            : 'Confirm interrupt: this sends the Agent adapter interrupt bytes '
                  'without terminating its Session.';
      });
      return;
    }
    final valid = globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
    );
    if (!valid) {
      setState(() {
        _armedControl = null;
        _sendFeedback =
            'Agent identity changed. Refresh status before controlling.';
      });
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostVerifiedAgentLifecycleClient) {
      setState(() {
        _armedControl = null;
        _sendFeedback =
            'Host does not support verified Agent lifecycle control.';
      });
      return;
    }
    setState(() {
      _controlling = true;
      _armedControl = null;
      _sendFeedback = 'Requesting verified Agent lifecycle action…';
    });
    try {
      await (host as TerminalHostVerifiedAgentLifecycleClient)
          .controlVerifiedAgent(
            sessionId: identity.sessionId,
            workspaceId: identity.workspaceId,
            tabId: identity.tabId,
            terminate: terminate,
          );
      if (!mounted) return;
      setState(() {
        if (terminate && _target == identity) {
          _target = null;
          _instruction.clear();
        }
        _sendFeedback = terminate
            ? 'Host confirmed Session termination. Refresh the Agent list.'
            : 'Host accepted Agent interrupt. Check its next Hook status.';
      });
    } catch (_) {
      if (!mounted) return;
      // Lost ACK may still mean the request was executed. Do not retry.
      setState(
        () => _sendFeedback = 'Lifecycle result unconfirmed. Inspect Host status before retrying.',
      );
    } finally {
      if (mounted) setState(() => _controlling = false);
    }
  }

  final Set<({String sessionId, String workspaceId, String tabId})>
  _hiddenCards = {};
  bool _showHidden = false;

  ({String sessionId, String workspaceId, String tabId}) _identity(
    GlobalAgentLiveRow row,
  ) => (
    sessionId: row.tab.terminalSessionId,
    workspaceId: row.status.workspaceId,
    tabId: row.tab.id,
  );

  void _toggleHidden(GlobalAgentLiveRow row) {
    final identity = _identity(row);
    setState(() {
      if (!_hiddenCards.add(identity)) _hiddenCards.remove(identity);
      // Hidden cards cannot silently retain input focus as the target.
      if (_target == identity) {
        _target = null;
        _instruction.clear();
      }
      _armedControl = null;
    });
  }

  @override
  void dispose() {
    _instruction.dispose();
    super.dispose();
  }

  Future<void> _submitToAgent() async {
    final target = _target;
    if (target == null || _sending) return;
    final instruction = _instruction.text;
    final bytes = utf8.encode(instruction).length;
    if (instruction.trim().isEmpty ||
        bytes > 4096 ||
        instruction.contains('\u0000')) {
      setState(() => _sendFeedback = 'Enter a prompt up to 4 KiB.');
      return;
    }
    final stillCurrent = globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: target.sessionId,
      workspaceId: target.workspaceId,
      tabId: target.tabId,
    );
    if (!stillCurrent) {
      setState(() => _sendFeedback = 'Agent target changed. Select it again.');
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostVerifiedAgentInputClient) {
      setState(
        () => _sendFeedback =
            'Current Host does not support guarded Agent input.',
      );
      return;
    }
    setState(() {
      _sending = true;
      _sendFeedback = 'Sending to the verified Host Session…';
    });
    try {
      await (host as TerminalHostVerifiedAgentInputClient)
          .submitVerifiedAgentPrompt(
            sessionId: target.sessionId,
            workspaceId: target.workspaceId,
            tabId: target.tabId,
            prompt: instruction,
          );
      if (!mounted) return;
      if (_target == target && _instruction.text == instruction) {
        _instruction.clear();
      }
      setState(
        () => _sendFeedback = _target == target
            ? 'Host acknowledged Agent input.'
            : 'Host acknowledged input to the previously selected Agent.',
      );
    } catch (_) {
      if (!mounted) return;
      // Do not retry automatically: a disconnect after enqueue could have
      // delivered the prompt despite a missing acknowledgement.
      setState(
        () => _sendFeedback =
            'Could not confirm delivery. Refresh status before retrying.',
      );
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _launch() async {
    final created = await showDialog<String>(
      context: context,
      builder: (_) => const GlobalAgentLaunchDialog(),
    );
    if (!mounted || created == null) return;
    setState(
      () => _sendFeedback = 'Agent tab created. New sessions appear after Host/Hook confirmation.',
    );
  }

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
    final liveIdentities = all.map(_identity).toSet();
    _hiddenCards.removeWhere((identity) => !liveIdentities.contains(identity));
    final rows = all
        .where((row) => _showHidden || !_hiddenCards.contains(_identity(row)))
        .where((row) => row.matches(_query))
        .toList();
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
                  TextButton.icon(
                    onPressed: _launch,
                    icon: const Icon(Icons.add),
                    label: const Text('New Agent'),
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
              Row(
                children: [
                  Expanded(
                    child: Text(
                      '${rows.length} of ${all.length} Agent sessions',
                      style: muted,
                    ),
                  ),
                  TextButton(
                    onPressed: () => setState(() => _showHidden = !_showHidden),
                    child: Text(
                      _showHidden
                          ? 'Show visible only'
                          : 'Show hidden cards (${_hiddenCards.length})',
                    ),
                  ),
                ],
              ),
              const SizedBox(height: AleraTokens.space12),
              if (_target != null) ...[
                Text(
                  'Target: ${_target!.workspaceId} / ${_target!.tabId}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: muted,
                ),
                const SizedBox(height: 6),
                Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: _instruction,
                        enabled: !_sending,
                        maxLines: 2,
                        minLines: 1,
                        maxLength: 4000,
                        decoration: const InputDecoration(
                          hintText: 'Send an instruction to this Agent without opening its Tab',
                          isDense: true,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    FilledButton(
                      onPressed: _sending ? null : _submitToAgent,
                      child: const Text('Send'),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
              ],
              if (_sendFeedback != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Text(_sendFeedback!, style: muted),
                ),
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
                                      onPressed: () => _toggleHidden(row),
                                      icon: Icon(
                                        _hiddenCards.contains(_identity(row))
                                            ? Icons.visibility
                                            : Icons.visibility_off,
                                      ),
                                      label: Text(
                                        _hiddenCards.contains(_identity(row))
                                            ? 'Show card'
                                            : 'Hide card (Agent keeps running)',
                                      ),
                                    ),
                                  ),
                                  Align(
                                    alignment: Alignment.centerRight,
                                    child: TextButton.icon(
                                      onPressed: () => setState(() {
                                        _target = (
                                          sessionId: row.tab.terminalSessionId,
                                          workspaceId: row.status.workspaceId,
                                          tabId: row.tab.id,
                                        );
                                        _sendFeedback = null;
                                      }),
                                      icon: const Icon(Icons.edit_note),
                                      label: const Text('Steer'),
                                    ),
                                  ),
                                  Row(
                                    mainAxisAlignment: MainAxisAlignment.end,
                                    children: [
                                      TextButton.icon(
                                        onPressed: _controlling || _sending
                                            ? null
                                            : () => _controlAgent(row, false),
                                        icon: const Icon(
                                          Icons.pause_circle_outline,
                                        ),
                                        label: Text(
                                          _armedControl ==
                                                  (
                                                    sessionId: row
                                                        .tab
                                                        .terminalSessionId,
                                                    workspaceId:
                                                        row.status.workspaceId,
                                                    tabId: row.tab.id,
                                                    terminate: false,
                                                  )
                                              ? 'Confirm interrupt'
                                              : 'Interrupt turn',
                                        ),
                                      ),
                                      TextButton.icon(
                                        onPressed: _controlling || _sending
                                            ? null
                                            : () => _controlAgent(row, true),
                                        icon: const Icon(
                                          Icons.stop_circle_outlined,
                                        ),
                                        label: Text(
                                          _armedControl ==
                                                  (
                                                    sessionId: row
                                                        .tab
                                                        .terminalSessionId,
                                                    workspaceId:
                                                        row.status.workspaceId,
                                                    tabId: row.tab.id,
                                                    terminate: true,
                                                  )
                                              ? 'Confirm terminate'
                                              : 'Terminate Session',
                                        ),
                                      ),
                                    ],
                                  ),
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
