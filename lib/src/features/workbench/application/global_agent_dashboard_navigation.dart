import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Window-local navigation state. Dashboard occupies the center workbench,
/// not a dialog or a persisted project/Tab selection.
final globalAgentDashboardNavigationProvider =
    NotifierProvider<GlobalAgentDashboardNavigation, bool>(
      GlobalAgentDashboardNavigation.new,
    );

class GlobalAgentDashboardNavigation extends Notifier<bool> {
  @override
  bool build() => false;

  void open() => state = true;

  void close() => state = false;
}

/// A live inspector borrows the existing renderer of exactly one PTY.
final globalAgentInspectionOpenProvider =
    NotifierProvider<GlobalAgentInspectionOpen, String?>(
      GlobalAgentInspectionOpen.new,
    );

class GlobalAgentInspectionOpen extends Notifier<String?> {
  @override
  String? build() => null;

  void open(String sessionId) => state = sessionId;

  void close() => state = null;
}
