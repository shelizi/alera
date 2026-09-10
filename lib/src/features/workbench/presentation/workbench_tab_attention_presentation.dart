import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_status/presentation/agent_status_dot.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_attention.dart';
import 'package:flutter/material.dart';

/// Dot color for the tab strip: unacked done uses warning amber.
Color? workbenchTabAttentionDotColor({
  required AgentStatusEntry? status,
  required bool completionAcknowledged,
}) {
  final entry = status;
  if (entry == null) {
    return null;
  }
  final attention = workbenchTabAttention(
    status: entry,
    completionAcknowledged: completionAcknowledged,
  );
  return switch (attention) {
    WorkbenchTabAttention.agentDoneUnacked => AleraTokens.warning,
    WorkbenchTabAttention.none ||
    WorkbenchTabAttention.agentWaiting ||
    WorkbenchTabAttention.agentBlocked => agentStatusColor(entry.state),
  };
}

String workbenchTabAttentionTooltip({
  required AgentStatusEntry status,
  required bool completionAcknowledged,
}) {
  final base = agentStatusTooltip(status);
  if (workbenchTabAttention(
        status: status,
        completionAcknowledged: completionAcknowledged,
      ) ==
      WorkbenchTabAttention.agentDoneUnacked) {
    return '$base (unacked)';
  }
  return base;
}
