import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Application-facing capability for directing focus to a terminal without
/// exposing presentation session handles or rendering APIs.
abstract interface class TerminalRuntimeFocus {
  void requestFocus({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  });
}
