import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_profiles/application/local_agent_providers.dart';
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
  String? _projectId;
  String? _workspaceId;
  AgentType? _agentType;
  final _projectFilter = TextEditingController();
  final _workspaceFilter = TextEditingController();
  final _agentFilter = TextEditingController();
  bool _creating = false;
  String? _error;

  @override
  void dispose() {
    _projectFilter.dispose();
    _workspaceFilter.dispose();
    _agentFilter.dispose();
    super.dispose();
  }

  Future<void> _create(Workspace workspace) async {
    final agentType = _agentType;
    if (_creating || agentType == null || workspace.projectId != _projectId)
      return;
    // A persisted workspace can be removed/replaced while the dialog is open.
    final state = ref.read(workbenchControllerProvider);
    final projectMatches = state.projects.any(
      (project) =>
          project.id == _projectId && project.name == _projectFilter.text,
    );
    final installed =
        ref.read(installedAgentClisProvider).asData?.value ??
        const <AgentType>[];
    final agentMatches =
        installed.contains(agentType) &&
        _agentFilter.text == agentDisplayName(agentType);
    final exists =
        projectMatches &&
        agentMatches &&
        workspace.name == _workspaceFilter.text &&
        state
            .workspacesFor(workspace.projectId)
            .any(
              (current) =>
                  current.id == workspace.id &&
                  current.isActive &&
                  current.path == workspace.path,
            );
    if (!exists) {
      setState(
        () => _error = 'The selected Workspace or Agent changed. Select again.',
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
          .executablePathFor(agentType.key);
      final tab = await ref
          .read(workbenchControllerProvider.notifier)
          .createAgentTab(
            workspace,
            agentType: agentType,
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
    final detected = ref.watch(installedAgentClisProvider);
    final agents = detected.asData?.value ?? const <AgentType>[];
    return ListenableBuilder(
      listenable: Listenable.merge([
        _projectFilter,
        _workspaceFilter,
        _agentFilter,
      ]),
      builder: (context, _) {
        final projects = state.projects
            .where(
              (project) => state
                  .workspacesFor(project.id)
                  .any((workspace) => workspace.isActive),
            )
            .toList();
        final project = projects
            .where(
              (project) =>
                  project.id == _projectId &&
                  project.name == _projectFilter.text,
            )
            .firstOrNull;
        final workspaces = project == null
            ? const <Workspace>[]
            : state
                  .workspacesFor(project.id)
                  .where((workspace) => workspace.isActive)
                  .toList();
        final active = workspaces
            .where(
              (workspace) =>
                  workspace.id == _workspaceId &&
                  workspace.name == _workspaceFilter.text,
            )
            .firstOrNull;
        final agentSelected =
            _agentType != null &&
            agents.contains(_agentType) &&
            _agentFilter.text == agentDisplayName(_agentType!);

        return AlertDialog(
          title: const Text('New Agent'),
          content: SizedBox(
            width: 460,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                DropdownMenu<String>(
                  key: const ValueKey('agent-create-project'),
                  controller: _projectFilter,
                  enabled: !_creating,
                  enableFilter: true,
                  enableSearch: true,
                  requestFocusOnTap: true,
                  expandedInsets: EdgeInsets.zero,
                  label: const Text('Project'),
                  dropdownMenuEntries: [
                    for (final entry in projects)
                      DropdownMenuEntry(value: entry.id, label: entry.name),
                  ],
                  onSelected: (value) => setState(() {
                    _projectId = value;
                    _workspaceId = null;
                    _agentType = null;
                    _workspaceFilter.clear();
                    _agentFilter.clear();
                    _error = null;
                  }),
                ),
                const SizedBox(height: 10),
                DropdownMenu<String>(
                  key: ValueKey(('agent-create-workspace', project?.id)),
                  controller: _workspaceFilter,
                  enabled: !_creating && project != null,
                  enableFilter: true,
                  enableSearch: true,
                  requestFocusOnTap: true,
                  expandedInsets: EdgeInsets.zero,
                  label: const Text('Workspace'),
                  dropdownMenuEntries: [
                    for (final entry in workspaces)
                      DropdownMenuEntry(value: entry.id, label: entry.name),
                  ],
                  onSelected: (value) => setState(() {
                    _workspaceId = value;
                    _agentType = null;
                    _agentFilter.clear();
                    _error = null;
                  }),
                ),
                const SizedBox(height: 10),
                DropdownMenu<AgentType>(
                  key: ValueKey(('agent-create-agent', active?.id)),
                  controller: _agentFilter,
                  enabled: !_creating && active != null && agents.isNotEmpty,
                  enableFilter: true,
                  enableSearch: true,
                  requestFocusOnTap: true,
                  expandedInsets: EdgeInsets.zero,
                  label: const Text('Agent'),
                  dropdownMenuEntries: [
                    for (final agent in agents)
                      DropdownMenuEntry(
                        value: agent,
                        label: agentDisplayName(agent),
                      ),
                  ],
                  onSelected: (value) => setState(() => _agentType = value),
                ),
                if (active != null && (agents.isEmpty || detected.isLoading))
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Row(
                      children: [
                        Expanded(
                          child: Text(
                            detected.isLoading
                                ? 'Detecting installed Agents…'
                                : detected.hasError
                                ? 'Agent detection failed.'
                                : 'No installed Agents detected. Check Settings.',
                          ),
                        ),
                        if (!detected.isLoading)
                          TextButton(
                            onPressed: _creating
                                ? null
                                : () => ref.invalidate(
                                    installedAgentClisProvider,
                                  ),
                            child: const Text('Refresh'),
                          ),
                      ],
                    ),
                  ),
                if (_error != null) ...[
                  const SizedBox(height: 12),
                  Text(
                    _error!,
                    style: const TextStyle(color: Colors.redAccent),
                  ),
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
              onPressed: _creating || active == null || !agentSelected
                  ? null
                  : () => _create(active),
              child: Text(_creating ? 'Creating…' : 'Create Agent'),
            ),
          ],
        );
      },
    );
  }
}
