part of 'terminal_runtime.dart';

/// Add Claude's opt-in Alera hook settings to an agent-tab startup only.
/// The real ~/.claude configuration remains active, including session history.
/// Arbitrary shell commands and manually configured launchers are untouched.
String? _canonicalClaudeStartupCommand({
  required String? initialCommand,
  required Object? agentType,
  required bool settingsEnabled,
  required Map<String, String>? environment,
  required String interactiveShell,
}) {
  if (!settingsEnabled || agentType != 'claude') return initialCommand;
  final original = initialCommand?.trim();
  if (original == null ||
      !RegExp(r'^claude(?:\s|$)').hasMatch(original) ||
      original.contains('--settings')) {
    return initialCommand;
  }
  final config = environment?['ALERA_CLAUDE_COPY_SETTINGS'];
  if (config == null || config.trim().isEmpty) return initialCommand;
  final shellName = p.basenameWithoutExtension(interactiveShell).toLowerCase();
  final quoted = switch (shellName) {
    'pwsh' || 'powershell' => "'" + config.replaceAll("'", "''") + "'",
    _ when Platform.isWindows => '"' + config.replaceAll('"', '\\"') + '"',
    _ => "'" + config.replaceAll("'", "'\\''") + "'",
  };
  return original + ' --settings ' + quoted;
}

Future<void> _deliverTerminalProcessStartup({
  required TerminalPtySession session,
  required GhosttyTerminalShellLaunch launch,
  required String interactiveShell,
  required String? initialCommand,
  required bool Function() isCurrent,
}) async {
  final setupCommand = launch.setupCommand;
  if (setupCommand != null && setupCommand.isNotEmpty) {
    await Future.pause(const Duration(milliseconds: 120));
    if (!isCurrent()) {
      return;
    }
    if (!await session.writeBytesAndWait(utf8.encode(setupCommand))) {
      return;
    }
    await Future.pause(const Duration(milliseconds: 120));
  }
  if (initialCommand == null || initialCommand.isEmpty) {
    return;
  }
  await Future.pause(const Duration(milliseconds: 120));
  if (!isCurrent()) {
    return;
  }
  if (shouldUseBracketedPasteForStartupCommand(initialCommand) &&
      terminalShellSupportsBracketedPaste(interactiveShell)) {
    if (!await session.writeBytesAndWait(
      buildAgentPromptPasteBytes(initialCommand),
    )) {
      return;
    }
    await Future.pause(terminalAgentPromptSubmitDelay);
    if (isCurrent()) {
      await session.writeBytesAndWait(utf8.encode('\r'));
    }
    return;
  }
  await session.writeBytesAndWait(
    buildPlainStartupCommandBytes(initialCommand),
  );
}
