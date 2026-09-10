import 'package:alera/src/features/command_terminal/application/command_terminal_session.dart';
import 'package:alera/src/features/command_terminal/domain/command_terminal_request.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';

/// Opens an ephemeral terminal session for [request].
///
/// The caller owns the session: it must call `runtime.closeTab(tabId)` when the
/// dialog goes away, which terminates the shell's whole process tree. Nothing
/// else will, because the workbench exit coordinator skips this workspace id.
TerminalSessionHandle openCommandTerminalSession({
  required TerminalRuntime runtime,
  required CommandTerminalRequest request,
  required String tabId,
  required String workingDirectory,
}) {
  return runtime.sessionFor(
    workspace: buildCommandTerminalWorkspace(
      workingDirectory: workingDirectory,
    ),
    tab: buildCommandTerminalTab(tabId: tabId, request: request),
  );
}
