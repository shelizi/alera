import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Ephemeral focus requests for the shared right-hand workspace search panel.
/// Kept separate from persisted search parameters and workbench view prefs.
final workspaceSearchFocusRequestProvider =
    NotifierProvider<WorkspaceSearchFocusRequest, int>(
      WorkspaceSearchFocusRequest.new,
    );

class WorkspaceSearchFocusRequest extends Notifier<int> {
  @override
  int build() => 0;

  void request() => state++;
}
