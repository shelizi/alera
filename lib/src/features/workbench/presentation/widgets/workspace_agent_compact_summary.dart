import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_run_groups.dart';
import 'package:alera/src/features/workbench/presentation/widgets/agent_run_spinner_scope.dart';
import 'package:flutter/material.dart';

/// Compact tray control for a workspace's agent runs: agents grouped by state,
/// each group showing its state glyph plus a run count. Clicking toggles the
/// expanded per-agent rows under the workspace.
class const WorkspaceAgentCompactSummary({
  super.key,
  required final List<WorkspaceAgentRunGroup> groups,
  required final bool expanded,
  required final VoidCallback onToggle,
  this.tooltipOverride,
}) extends StatelessWidget {
  /// Optional tooltip; defaults to Show/Hide Agent Runs.
  final String? tooltipOverride;

  @override
  Widget build(BuildContext context) {
    final tooltip =
        tooltipOverride ?? (expanded ? 'Hide Agent Runs' : 'Show Agent Runs');
    return Tooltip(
      message: tooltip,
      child: InkWell(
        onTap: onToggle,
        mouseCursor: SystemMouseCursors.click,
        borderRadius: .circular(AleraTokens.radiusSm),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: AleraTokens.space4,
            vertical: AleraTokens.space2,
          ),
          child: Row(
            mainAxisSize: .min,
            children: <Widget>[
              for (final (index, group) in groups.indexed) ...<Widget>[
                if (index > 0) const SizedBox(width: AleraTokens.space6),
                WorkspaceAgentGroupCount(
                  kind: group.kind,
                  count: group.runs.length,
                ),
              ],
              const SizedBox(width: AleraTokens.space2),
              Icon(
                expanded ? AleraIcons.chevronUp : AleraIcons.chevronDown,
                size: 12,
                color: AleraTokens.foregroundMuted,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// State glyph for one [WorkspaceAgentGroupKind]. The compact tray and the
/// project header badge share this mapping so a count beside a glyph means
/// the same thing in both places.
class const WorkspaceAgentGroupGlyph({
  super.key,
  required final WorkspaceAgentGroupKind kind,
  this.size = 11,
}) extends StatelessWidget {
  final double size;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: Center(child: _buildGlyph(context)),
    );
  }

  Widget _buildGlyph(BuildContext context) {
    if (kind == WorkspaceAgentGroupKind.working) {
      if (AgentRunSharedSpinner.isAvailable(context)) {
        return AgentRunSharedSpinner(
          size: size - 2,
          color: AleraTokens.warning,
          strokeWidth: 1.7,
        );
      }
      return SizedBox.square(
        dimension: size - 2,
        child: const CircularProgressIndicator(
          strokeWidth: 1.7,
          color: AleraTokens.warning,
        ),
      );
    }
    final (icon, color) = switch (kind) {
      WorkspaceAgentGroupKind.waiting => (
        AleraIcons.notifications,
        AleraTokens.warning,
      ),
      WorkspaceAgentGroupKind.blocked => (
        AleraIcons.notifications,
        AleraTokens.error,
      ),
      WorkspaceAgentGroupKind.interrupted => (
        AleraIcons.cancel,
        AleraTokens.error,
      ),
      // An unread completion is still an attention state, so it borrows the
      // warning tint instead of the done green.
      WorkspaceAgentGroupKind.doneUnacked => (
        AleraIcons.success,
        AleraTokens.warning,
      ),
      WorkspaceAgentGroupKind.working => (
        AleraIcons.sync,
        AleraTokens.warning,
      ), // coverage:ignore-line
      WorkspaceAgentGroupKind.done => (AleraIcons.success, AleraTokens.success),
    };
    return Icon(icon, size: size, color: color);
  }
}

/// One [WorkspaceAgentGroupKind] glyph plus its run count.
class const WorkspaceAgentGroupCount({
  super.key,
  required final WorkspaceAgentGroupKind kind,
  required final int count,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      mainAxisSize: .min,
      children: <Widget>[
        WorkspaceAgentGroupGlyph(kind: kind),
        const SizedBox(width: AleraTokens.space2),
        Text(
          '$count',
          style: theme.textTheme.labelSmall?.copyWith(
            color: AleraTokens.foregroundMuted,
            fontWeight: .w600,
          ),
        ),
      ],
    );
  }
}

/// Tooltip label for a group kind; mirrors the per-run state label.
String workspaceAgentGroupLabel(WorkspaceAgentGroupKind kind) {
  return switch (kind) {
    WorkspaceAgentGroupKind.waiting => 'Waiting for input',
    WorkspaceAgentGroupKind.blocked => 'Blocked',
    WorkspaceAgentGroupKind.interrupted => 'Interrupted',
    WorkspaceAgentGroupKind.doneUnacked => 'Done (Unread)',
    WorkspaceAgentGroupKind.working => 'Working',
    WorkspaceAgentGroupKind.done => 'Done',
  };
}
