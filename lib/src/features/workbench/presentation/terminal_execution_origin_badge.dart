import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/material.dart';

/// Displays the *execution* identity separately from the workspace which
/// currently owns the tab. Moving a live PTY does not cd its shell, rewrite
/// inherited environment variables, or retarget an already running agent.
class TerminalExecutionOriginBadge extends StatelessWidget {
  const TerminalExecutionOriginBadge({super.key, required this.tab});

  final WorkspaceTabRecord tab;

  @override
  Widget build(BuildContext context) {
    final original = tab.payload['terminalExecutionWorkspaceId'];
    if (tab.kind != WorkspaceTabKind.terminal ||
        original is! String ||
        original.isEmpty ||
        original == tab.workspaceId) {
      return const SizedBox.shrink();
    }
    final initialDirectory = tab.payload['terminalExecutionWorkingDirectory'];
    final directory = initialDirectory is String && initialDirectory.isNotEmpty
        ? '\nOriginal launch directory: $initialDirectory'
        : '';
    return Tooltip(
      message:
          'This terminal was started in workspace $original.$directory\n'
          'Moving the tab does not change the running shell\'s directory, '
          'environment, or agent execution context.',
      child: const Padding(
        padding: EdgeInsets.only(right: AleraTokens.space4),
        child: Icon(
          AleraIcons.warning,
          size: 12,
          color: AleraTokens.foregroundMuted,
        ),
      ),
    );
  }
}
