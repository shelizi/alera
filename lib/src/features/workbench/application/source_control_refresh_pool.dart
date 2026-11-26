import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Shared within the app's provider scope, not process-global. Only automatic
/// Source Control snapshot reloads consume slots; initial/explicit loads and
/// mutations must never wait behind unrelated worktrees' watcher traffic.
final sourceControlRefreshPoolProvider = Provider<WorkbenchBackgroundTaskPool>(
  (ref) => WorkbenchBackgroundTaskPool(maxConcurrent: 4),
);
