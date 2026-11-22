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

/// Temporarily unmounts the background workbench's TerminalView while a
/// live Session inspector owns that very same renderer. Host PTYs and all
/// TerminalSessionHandle objects remain alive; only the duplicate view goes.
final globalAgentInspectionOpenProvider =
    NotifierProvider<GlobalAgentInspectionOpen, bool>(
      GlobalAgentInspectionOpen.new,
    );

class GlobalAgentInspectionOpen extends Notifier<bool> {
  @override
  bool build() => false;

  void open() => state = true;

  void close() => state = false;
}
