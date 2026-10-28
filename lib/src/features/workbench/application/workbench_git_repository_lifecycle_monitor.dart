import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:path/path.dart' as p;

/// Watches the project root for `.git` directory/file creation and removal.
/// Rechecks the actual repository rather than trusting an intermediate event.
final class WorkbenchGitRepositoryLifecycleMonitor {
  WorkbenchGitRepositoryLifecycleMonitor({
    required Future<void> Function(String projectId) refreshProject,
    Stream<String> Function(String path)? watchRoot,
    void Function(Object error)? onError,
    Duration debounce = const Duration(milliseconds: 650),
  }) : _refreshProject = refreshProject,
       _watchRoot = watchRoot ?? _watchProjectRoot,
       _onError = onError,
       _debounce = debounce;

  final Future<void> Function(String projectId) _refreshProject;
  final Stream<String> Function(String path) _watchRoot;
  final void Function(Object error)? _onError;
  final Duration _debounce;

  Project? _project;
  StreamSubscription<String>? _subscription;
  Timer? _debounceTimer;
  bool _disposed = false;

  static Stream<String> _watchProjectRoot(String path) =>
      Directory(path).watch(recursive: false).map((event) => event.path);

  void selectProject(Project? project) {
    if (_disposed) return;
    if (_project?.id == project?.id &&
        _project?.repoPath == project?.repoPath) {
      _project = project;
      return;
    }
    _debounceTimer?.cancel();
    _debounceTimer = null;
    unawaited(_subscription?.cancel());
    _subscription = null;
    _project = project;
    if (project == null) return;

    try {
      _subscription = _watchRoot(project.repoPath).listen((path) {
        if (!isGitMetadataEntryEvent(project.repoPath, path)) return;
        _debounceTimer?.cancel();
        _debounceTimer = Timer(_debounce, _refreshBestEffort);
      }, onError: (Object error) => _onError?.call(error));
    } catch (error) {
      _onError?.call(error);
    }
    // Correct an already-stale persisted kind when a project is selected.
    _refreshBestEffort();
  }

  void onAppResumed() {
    if (!_disposed && _project != null) _refreshBestEffort();
  }

  Future<void> refresh() async {
    final project = _project;
    if (_disposed || project == null) return;
    await _refreshProject(project.id);
  }

  void _refreshBestEffort() {
    unawaited(
      refresh().catchError((Object error) {
        _onError?.call(error);
      }),
    );
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _debounceTimer?.cancel();
    _debounceTimer = null;
    unawaited(_subscription?.cancel());
    _subscription = null;
    _project = null;
  }
}

/// Supports both `.git` directories and `.git` worktree pointer files;
/// unrelated nested repositories should not change the parent project's kind.
bool isGitMetadataEntryEvent(String projectRoot, String eventPath) {
  final normalized = p.normalize(eventPath);
  return p.basename(normalized).toLowerCase() == '.git' &&
      p.equals(p.normalize(projectRoot), p.dirname(normalized));
}
