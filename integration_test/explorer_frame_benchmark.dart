/// Real Windows Profile/AOT frame benchmark: production WorkspaceExplorer +
/// Rust FFI with synthetic listings/watch events. Run in isolated worktree:
///   ALERA_E2E_DESKTOP=1 flutter drive --profile -d windows --no-pub
///     --target integration_test/explorer_frame_benchmark.dart
///     --driver test_driver/explorer_frame_benchmark_test.dart
library;

import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/presentation/workspace_explorer.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/rust/frb_generated.dart' show RustLib;
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import '../test/unit/fake_git_backend.dart';

const _sizes = <int>[100, 300, 1000];
const _batch = 150;
const _cycles = 5;
const _poll = Duration(milliseconds: 30);

native.WorkspaceFileEntry _file(String name, {bool modified = false}) =>
    native.WorkspaceFileEntry(
      relativePath: name,
      name: name,
      kind: native.WorkspaceFileKind.file,
      size: BigInt.from(modified ? 50 : 0),
      modifiedMillis: modified ? 2 : 1,
      contentToken: modified ? 'changed' : 'original',
      isIgnored: false,
      isHidden: false,
      isSymlink: false,
      isProtected: false,
      hasChildrenHint: false,
      gitStatus: modified ? native.WorkspaceFileGitStatus.modified : null,
    );

class _ProfileFileService extends WorkspaceFileService {
  final StreamController<native.WorkspaceExplorerWatchBatch> _events =
      StreamController<native.WorkspaceExplorerWatchBatch>.broadcast(
        sync: true,
      );
  List<native.WorkspaceFileEntry> _root = [];
  int completedUpdates = 0;
  int nativeCalls = 0;
  int fallbackCalls = 0;

  void configure(int count, {bool extra = false, bool modified = false}) {
    _root = [
      for (var i = 0; i < count; i++)
        _file(
          'file_${i.toString().padLeft(5, '0')}.dart',
          modified: modified && i == 0,
        ),
      if (extra)
        for (var i = 0; i < _batch; i++)
          _file('batch_${i.toString().padLeft(3, '0')}.dart'),
    ];
  }

  void notifyChanged() => _events.add(
    const native.WorkspaceExplorerWatchBatch(
      directoryRelativePaths: [''],
      changedRelativePaths: [],
      coalescedEventCount: 1,
    ),
  );

  @override
  Future<List<native.WorkspaceFileEntry>> listChildren({
    required String workspacePath,
    required String relativePath,
    required bool hideIgnored,
    bool hideHidden = false,
  }) async => relativePath.isEmpty
      ? List<native.WorkspaceFileEntry>.unmodifiable(_root)
      : const <native.WorkspaceFileEntry>[];

  @override
  Future<native.WorkspaceExplorerWatcherHandle> startExplorerWatcher({
    required String workspacePath,
  }) async => const native.WorkspaceExplorerWatcherHandle(id: 'isolated-gpu');

  @override
  Stream<native.WorkspaceExplorerWatchBatch> watchExplorerEvents({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) => _events.stream;

  @override
  Future<void> updateExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
    required List<String> watchedRelativePaths,
  }) async {}

  @override
  Future<void> stopExplorerWatcher({
    required native.WorkspaceExplorerWatcherHandle handle,
  }) async {}

  @override
  void scheduleQuickOpenRefresh({
    required String workspacePath,
    Duration debounce = const Duration(milliseconds: 120),
  }) {}

  @override
  Future<native.WorkspaceExplorerProjectionDelta>
  updateExplorerProjectionSession({
    required native.WorkspaceExplorerProjectionHandle handle,
    required native.WorkspaceExplorerDirectoryChildren replacement,
  }) async {
    nativeCalls++;
    final result = await super.updateExplorerProjectionSession(
      handle: handle,
      replacement: replacement,
    );
    completedUpdates++;
    return result;
  }

  @override
  Future<native.WorkspaceExplorerTreeProjection> projectExplorerTree({
    required String workspaceName,
    required String workspacePath,
    required List<native.WorkspaceExplorerDirectoryChildren> directories,
    native.WorkspaceExplorerDirectoryChildren? replacement,
  }) {
    fallbackCalls++;
    return super.projectExplorerTree(
      workspaceName: workspaceName,
      workspacePath: workspacePath,
      directories: directories,
      replacement: replacement,
    );
  }

  @override
  Future<void> dispose() async {
    await _events.close();
    await super.dispose();
  }
}

double _p(List<double> xs, double q) {
  if (xs.isEmpty) return 0;
  final copy = [...xs]..sort();
  return copy[((copy.length - 1) * q).round()];
}

Map<String, Object> _stats(List<double> xs) => {
  'count': xs.length,
  'p50_ms': _p(xs, 0.5),
  'p95_ms': _p(xs, 0.95),
  'max_ms': _p(xs, 1),
};

class _Samples {
  _Samples(this.name);
  final String name;
  final List<double> latency = [];
  final List<FrameTiming> frames = [];
  Map<String, Object> report() {
    final build = [
      for (final f in frames) f.buildDuration.inMicroseconds / 1000,
    ];
    final raster = [
      for (final f in frames) f.rasterDuration.inMicroseconds / 1000,
    ];
    final total = [for (final f in frames) f.totalSpan.inMicroseconds / 1000];
    return {
      'scenario': name,
      'wall': _stats(latency),
      'build': _stats(build),
      'raster': _stats(raster),
      'total': _stats(total),
      'frames_over_16_7ms': total.where((x) => x > 16.7).length,
      'frames_over_33_3ms': total.where((x) => x > 33.3).length,
    };
  }
}

Future<void> _awaitProjection(
  WidgetTester tester,
  _ProfileFileService service,
  int expected,
  int before,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  var actual = -1;
  while (DateTime.now().isBefore(deadline)) {
    await tester.pump(_poll);
    final view = find.byType(tree.DirectoryTreeView);
    if (view.evaluate().isNotEmpty) {
      actual = tester
          .widget<tree.DirectoryTreeView>(view)
          .controller
          .data
          .nodes
          .length;
      if (actual == expected && service.completedUpdates > before) return;
    }
    await Future<void>.delayed(_poll);
  }
  fail('Expected projected nodes $expected, observed $actual');
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'Explorer Windows Profile structural and metadata frame timings',
    (tester) async {
      binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;
      await RustLib.init();
      final temp = await Directory.systemTemp.createTemp('explorer_gpu_bench_');
      addTearDown(() => temp.delete(recursive: true));
      final reports = <Map<String, Object>>[];

      for (final count in _sizes) {
        final service = _ProfileFileService()..configure(count);
        final sessions = EditorSessionRegistry();
        final date = DateTime.now();
        await tester.pumpWidget(
          ProviderScope(
            key: ValueKey('explorer-$count'),
            overrides: [
              settingsControllerProvider.overrideWithValue(
                AleraSettings.defaults,
              ),
              workspaceFileServiceProvider.overrideWithValue(service),
              editorSessionRegistryProvider.overrideWithValue(sessions),
              gitBackendProvider.overrideWithValue(
                FakeGitBackend()..isRepository = false,
              ),
            ],
            child: MaterialApp(
              home: Scaffold(
                body: SizedBox(
                  width: 500,
                  height: 720,
                  child: WorkspaceExplorer(
                    workspace: Workspace(
                      id: 'profile-$count',
                      projectId: 'profile',
                      name: 'Explorer benchmark',
                      path: temp.path,
                      createdAt: date,
                      updatedAt: date,
                      kind: WorkspaceKind.main,
                      status: WorkspaceStatus.active,
                    ),
                    mode: WorkspaceExplorerMode.hideIgnored,
                    onModeChanged: (_) {},
                    showHiddenFiles: false,
                    onShowHiddenFilesChanged: (_) {},
                    onOpenFile: (_) {},
                    onPathMoved: (_, _) async {},
                  ),
                ),
              ),
            ),
          ),
        );
        await _awaitProjection(tester, service, count + 1, -1);
        await tester.pumpAndSettle();

        final add = _Samples('add_150');
        final remove = _Samples('remove_150');
        final metadata = _Samples('metadata_only');

        Future<void> sample(
          _Samples sink, {
          required bool extra,
          bool modified = false,
        }) async {
          final before = service.completedUpdates;
          service.configure(count, extra: extra, modified: modified);
          final frames = <FrameTiming>[];
          void collect(List<FrameTiming> timings) => frames.addAll(timings);
          binding.addTimingsCallback(collect);
          final elapsed = Stopwatch()..start();
          try {
            service.notifyChanged();
            await _awaitProjection(
              tester,
              service,
              count + 1 + (extra ? _batch : 0),
              before,
            );
            elapsed.stop();
            await tester.pump();
            await Future<void>.delayed(const Duration(milliseconds: 80));
          } finally {
            elapsed.stop();
            binding.removeTimingsCallback(collect);
          }
          sink.latency.add(elapsed.elapsedMicroseconds / 1000);
          sink.frames.addAll(frames);
        }

        await sample(add, extra: true); // warmup
        await sample(remove, extra: false);
        add.latency.clear();
        add.frames.clear();
        remove.latency.clear();
        remove.frames.clear();
        for (var i = 0; i < _cycles; i++) {
          await sample(add, extra: true);
          await sample(remove, extra: false);
          if (i.isEven) {
            await sample(metadata, extra: false, modified: true);
            await sample(metadata, extra: false);
          }
        }
        expect(
          service.fallbackCalls,
          0,
          reason: 'Must exercise real native versioned Rust projection session',
        );
        final report = <String, Object>{
          'rootFiles': count,
          'batchFiles': _batch,
          'nativeCalls': service.nativeCalls,
          'fallbackCalls': service.fallbackCalls,
          'scenarios': [add.report(), remove.report(), metadata.report()],
        };
        reports.add(report);
        // ignore: avoid_print
        print('EXPLORER_PROFILE $report');
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.pump();
        await service.dispose();
        sessions.dispose();
      }
      binding.reportData = {
        'benchmark': 'explorer_windows_profile_aot',
        'mode': kProfileMode ? 'profile' : (kReleaseMode ? 'release' : 'debug'),
        'os': Platform.operatingSystemVersion,
        'scenario': 'Real WorkspaceExplorer UI + Rust FFI; synthetic root listing/watch. No disk/Git I/O.',
        'fixtures': reports,
        'cycles': _cycles,
      };
      expect(reports, hasLength(_sizes.length));
    },
    timeout: const Timeout(Duration(minutes: 12)),
  );
}
