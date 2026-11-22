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
