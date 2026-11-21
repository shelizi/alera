import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Creates a Host-owned Agent tab in the explicitly chosen registered
/// worktree. This reuses Classic's existing launch/credentials policy.
class const GlobalAgentLaunchDialog({super.key})
    extends ConsumerStatefulWidget {
  @override
  ConsumerState<GlobalAgentLaunchDialog> createState() =>
      _GlobalAgentLaunchDialogState();
}

class _GlobalAgentLaunchDialogState
    extends ConsumerState<GlobalAgentLaunchDialog> {
  String? _workspaceId;
  AgentType _agentType = AgentType.codex;
  bool _creating = false;
  String? _error;

  Future<void> _create(Workspace workspace) async {
    if (_creating) return;
    // A persisted workspace can be removed/replaced while the dialog is open.
    final state = ref.read(workbenchControllerProvider);
    final exists = state
        .workspacesFor(workspace.projectId)
        .any(
          (current) =>
              current.id == workspace.id &&
              current.isActive &&
              current.path == workspace.path,
        );
    if (!exists) {
      setState(
        () => _error = 'The selected workspace changed. Refresh and retry.',
      );
      return;
    }
    setState(() {
      _creating = true;
      _error = null;
    });
    try {
      final executable = ref
          .read(settingsControllerProvider)
          .agents
          .executablePathFor(_agentType.key);
      final tab = await ref
          .read(workbenchControllerProvider.notifier)
          .createAgentTab(
            workspace,
            agentType: _agentType,
            executablePath: executable,
          );
      if (mounted) Navigator.of(context).pop(tab.id);
    } catch (_) {
      if (mounted) {
        setState(
          () => _error = 'Could not confirm Agent creation. Check this workspace before retrying.',
        );
      }
    } finally {
      if (mounted) setState(() => _creating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(workbenchControllerProvider);
    final choices = <({Workspace workspace, String label})>[
      for (final project in state.projects)
        for (final workspace in state.workspacesFor(project.id))
          if (workspace.isActive)
            (
              workspace: workspace,
              label: '${project.name} / ${workspace.name}',
            ),
    ];
    final selected = choices
        .where((entry) => entry.workspace.id == _workspaceId)
        .firstOrNull;
    final active =
        selected ??
        choices
            .where((entry) => entry.workspace.id == state.activeWorkspaceId)
            .firstOrNull ??
        choices.firstOrNull;

    return AlertDialog(
      title: const Text('Create Agent in Workspace'),
      content: SizedBox(
        width: 460,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Creates a real Agent tab in the selected worktree. '
              'The current Dashboard stays open.',
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              key: ValueKey(active?.workspace.id),
              initialValue: active?.workspace.id,
              isExpanded: true,
              decoration: const InputDecoration(
                labelText: 'Project / Worktree',
              ),
              items: [
                for (final choice in choices)
                  DropdownMenuItem<String>(
                    value: choice.workspace.id,
                    child: Text(
                      choice.label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
              onChanged: _creating
                  ? null
                  : (value) => setState(() => _workspaceId = value),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<AgentType>(
              key: ValueKey(_agentType),
              initialValue: _agentType,
              decoration: const InputDecoration(labelText: 'Agent'),
              items: const [
                DropdownMenuItem(value: AgentType.codex, child: Text('Codex')),
                DropdownMenuItem(
                  value: AgentType.claude,
                  child: Text('Claude'),
                ),
              ],
              onChanged: _creating
                  ? null
                  : (value) {
                      if (value != null) setState(() => _agentType = value);
                    },
            ),
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(_error!, style: const TextStyle(color: Colors.redAccent)),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _creating ? null : () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _creating || active == null
              ? null
              : () => _create(active.workspace),
          child: Text(_creating ? 'Creating…' : 'Create Agent'),
        ),
      ],
    );
  }
}
