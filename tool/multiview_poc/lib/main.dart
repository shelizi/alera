import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:multiview_desktop/multiview_desktop.dart';
import 'package:window_manager/window_manager.dart' as wm;

import 'handoff_harness.dart';

void trace(String message) {
  File('poc_startup_trace.txt').writeAsStringSync(
    '${DateTime.now().toIso8601String()} pid=$pid $message\n',
    mode: FileMode.append,
    flush: true,
  );
}

// Shared instance: when opening child windows there is no Dart serialization.
final sharedCounter = ValueNotifier<int>(0);
final inputValuesByViewId = <int, String>{};

void main(List<String> args) {
  // Only the dedicated anchor-preservation experiment uses this unstable API.
  // ignore: experimental_member_use
  const preserveIndependentViews = CloseMode.none;
  trace('DART main args=$args');
  FlutterError.onError = (details) {
    trace('FLUTTER_ERROR ${details.exceptionAsString()}');
  };
  runMultiApp(
    home: (_, viewId) => MaterialApp(
      title: 'Alera single-engine PoC',
      theme: ThemeData.dark(useMaterial3: true),
      home: args.contains('--poc-handoff')
          ? HandoffHarness(
              mainViewId: viewId,
              cycles: args.contains('--poc-handoff-stress') ? 20 : 8,
              requireExternalWmClose: args.contains('--poc-handoff-wmclose'),
            )
          : _MainView(
              autoSmoke: args.contains('--poc-smoke'),
              pluginProbe: args.contains('--poc-window-manager'),
              displayProbe: args.contains('--poc-display'),
              anchorProbe:
                  args.contains('--poc-anchor-close') ||
                  args.contains('--poc-anchor-keep'),
              keepAnchorChildren: args.contains('--poc-anchor-keep'),
              inputProbe: args.contains('--poc-input'),
              cycleCount: args.contains('--poc-long')
                  ? 200
                  : args.contains('--poc-stress')
                  ? 50
                  : 10,
            ),
    ),
    config: MultiAppConfig(
      observers: <WindowObserver>[handoffWindowObserver],
      fileLogParams: const LogParams(enable: true),
      generalParams: MultiPlatformParams(
        closeMode: args.contains('--poc-anchor-keep')
            ? preserveIndependentViews
            : CloseMode.softCascade,
        enableDynamicAnchor: args.contains('--poc-anchor-keep'),
        closeAppAfterLastWindowClosed: true,
        animation: ViewAnimationConfig.disabled,
      ),
    ),
  );
}

class _MainView extends StatefulWidget {
  const _MainView({
    required this.autoSmoke,
    required this.cycleCount,
    required this.pluginProbe,
    required this.displayProbe,
    required this.anchorProbe,
    required this.keepAnchorChildren,
    required this.inputProbe,
  });
  final bool autoSmoke;
  final int cycleCount;
  final bool pluginProbe;
  final bool displayProbe;
  final bool anchorProbe;
  final bool keepAnchorChildren;
  final bool inputProbe;
  @override
  State<_MainView> createState() => _MainViewState();
}

class _MainViewState extends State<_MainView> {
  String status = 'Idle';
  bool started = false;

  @override
  void initState() {
    super.initState();
    trace('DART main-view initState autoSmoke=${widget.autoSmoke}');
    if (widget.autoSmoke && !started) {
      started = true;
      WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(_smoke()));
    } else if (widget.pluginProbe) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => unawaited(_pluginProbe()),
      );
    } else if (widget.displayProbe) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => unawaited(_displayProbe()),
      );
    } else if (widget.anchorProbe) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => unawaited(_anchorCloseProbe()),
      );
    } else if (widget.inputProbe) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => unawaited(_inputProbe()),
      );
    }
  }

  Future<void> _smoke() async {
    trace('DART smoke started');
    final rows = <Map<String, Object?>>[];
    final report = <String, Object?>{
      'processId': pid,
      'architecture': 'one_process_one_engine_many_views',
      'cycles': rows,
      'passed': false,
    };
    try {
      await Future<void>.delayed(const Duration(milliseconds: 350));
      for (var index = 0; index < widget.cycleCount; index++) {
        final initialViews =
            WidgetsBinding.instance.platformDispatcher.views.length;
        final startedAt = Stopwatch()..start();
        final firstFrame = Completer<Map<String, Object?>>();
        final id = await openWindow(
          (_, viewId) => _ChildView(
            index: index,
            id: viewId,
            onFirstFrame: (devicePixelRatio, physicalSize) {
              if (!firstFrame.isCompleted) {
                firstFrame.complete({
                  'callbackMs': startedAt.elapsedMicroseconds / 1000.0,
                  'devicePixelRatio': devicePixelRatio,
                  'physicalWidth': physicalSize.width,
                  'physicalHeight': physicalSize.height,
                });
              }
            },
          ),
          options: WindowOptions(
            title: 'Alera PoC $index',
            size: const Size(480, 320),
          ),
        );
        final createMs = startedAt.elapsedMicroseconds / 1000.0;
        final frame = await firstFrame.future.timeout(
          const Duration(seconds: 4),
          onTimeout: () => throw StateError(
            'child first-frame callback timeout cycle=$index id=$id',
          ),
        );
        await Future<void>.delayed(const Duration(milliseconds: 150));
        final openedViews =
            WidgetsBinding.instance.platformDispatcher.views.length;
        final knownId = MultiViewDesktop.allWindowViewIds.contains(id);
        final sharedValue = sharedCounter.value;
        if (!knownId ||
            openedViews <= initialViews ||
            sharedValue < index + 1) {
          throw StateError(
            'create $index: id=$knownId views=$openedViews counter=$sharedValue',
          );
        }
        startedAt.reset();
        await MultiViewDesktop.fromId(id).closeWindow();
        final closeMs = startedAt.elapsedMilliseconds;
        await Future<void>.delayed(const Duration(milliseconds: 60));
        final finalViews =
            WidgetsBinding.instance.platformDispatcher.views.length;
        if (MultiViewDesktop.allWindowViewIds.contains(id) ||
            finalViews != initialViews) {
          throw StateError('close $index: $initialViews -> $finalViews');
        }
        rows.add({
          'cycle': index,
          'viewId': id,
          'createMs': createMs,
          'dartFirstFrameCallbackMs': frame['callbackMs'],
          'devicePixelRatio': frame['devicePixelRatio'],
          'physicalSize': [frame['physicalWidth'], frame['physicalHeight']],
          'closeMs': closeMs,
          'visibleViewCount': openedViews,
          'sharedCounter': sharedValue,
          'residentBytes': ProcessInfo.currentRss,
        });
        if (mounted) {
          setState(
            () => status = 'Cycle ${index + 1}/${widget.cycleCount} passed',
          );
        }
      }
      // Verify three native OS windows coexist on this same engine.
      final beforeParallel =
          WidgetsBinding.instance.platformDispatcher.views.length;
      final parallelIds = <int>[];
      for (var i = 0; i < 3; i++) {
        final id = await openWindow(
          (_, viewId) => _ChildView(index: widget.cycleCount + i, id: viewId),
          options: WindowOptions(
            title: 'Parallel PoC $i',
            size: const Size(480, 320),
          ),
        );
        parallelIds.add(id);
      }
      await Future<void>.delayed(const Duration(milliseconds: 280));
      final duringParallel =
          WidgetsBinding.instance.platformDispatcher.views.length;
      final sharedAtParallel = sharedCounter.value;
      if (duringParallel != beforeParallel + 3 ||
          sharedAtParallel != widget.cycleCount + 3 ||
          parallelIds.any(
            (id) => !MultiViewDesktop.allWindowViewIds.contains(id),
          )) {
        throw StateError(
          'parallel: views=$duringParallel shared=$sharedAtParallel ids=$parallelIds',
        );
      }
      for (final id in parallelIds.reversed) {
        await MultiViewDesktop.fromId(id).closeWindow();
      }
      await Future<void>.delayed(const Duration(milliseconds: 180));
      final afterParallel =
          WidgetsBinding.instance.platformDispatcher.views.length;
      if (afterParallel != beforeParallel) {
        throw StateError(
          'parallel cleanup: expected $beforeParallel got $afterParallel',
        );
      }
      report['parallelTest'] = {
        'opened': parallelIds.length,
        'peakViews': duringParallel,
        'finalViews': afterParallel,
        'sharedCounter': sharedAtParallel,
      };
      trace('DART parallel windows passed count=3 peakViews=$duringParallel');
      report['passed'] = true;
    } catch (error, stack) {
      trace('DART smoke error=$error stack=$stack');
      report['error'] = '$error\n$stack';
      if (mounted) setState(() => status = 'FAIL $error');
    } finally {
      report['finalViews'] =
          WidgetsBinding.instance.platformDispatcher.views.length;
      report['finalCounter'] = sharedCounter.value;
      report['finalResidentBytes'] = ProcessInfo.currentRss;
      await File('poc_multiview_results.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
      trace('DART smoke report written');
      if (widget.autoSmoke) {
        await Future<void>.delayed(const Duration(milliseconds: 250));
        await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
      }
    }
  }

  List<Map<String, Object?>> _viewsSnapshot() => WidgetsBinding
      .instance
      .platformDispatcher
      .views
      .map(
        (view) => <String, Object?>{
          'viewId': view.viewId,
          'devicePixelRatio': view.devicePixelRatio,
          'physicalSize': [view.physicalSize.width, view.physicalSize.height],
        },
      )
      .toList();

  Future<void> _inputProbe() async {
    final report = <String, Object?>{
      'processId': pid,
      'scenario': 'native_keyboard_input_routed_to_two_child_views',
      'complete': false,
    };
    final ids = <int>[];
    try {
      for (var index = 0; index < 2; index++) {
        ids.add(
          await openWindow(
            (_, viewId) => _ChildView(index: 910 + index, id: viewId),
            options: WindowOptions(
              title: 'MVD_INPUT_WINDOW_$index',
              size: const Size(560, 360),
            ),
          ),
        );
      }
      trace('INPUT_PROBE windows-ready ids=$ids');
      await Future<void>.delayed(const Duration(seconds: 7));
      report['childViewIds'] = ids;
      report['capturedInput'] = ids
          .map((id) => {'id': id, 'text': inputValuesByViewId[id] ?? ''})
          .toList();
      report['expected'] = ['ALERAA', 'ALERAB'];
      report['routingPassed'] =
          inputValuesByViewId[ids[0]] == 'ALERAA' &&
          inputValuesByViewId[ids[1]] == 'ALERAB';
      report['complete'] = true;
    } catch (error, stack) {
      report['error'] = '$error\n$stack';
    } finally {
      await File('poc_input_probe.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
      for (final id in ids.reversed) {
        if (MultiViewDesktop.allWindowViewIds.contains(id)) {
          await MultiViewDesktop.fromId(id).closeWindow();
        }
      }
      await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
    }
  }

  Future<void> _anchorCloseProbe() async {
    final report = <String, Object?>{
      'processId': pid,
      'scenario': widget.keepAnchorChildren
          ? 'main_close_keep_children'
          : 'main_close_default_cascade',
      'afterPrimaryCloseObserved': false,
    };
    try {
      final children = <int>[];
      for (var index = 0; index < 2; index++) {
        final id = await openWindow(
          (_, viewId) => _ChildView(index: 880 + index, id: viewId),
          options: WindowOptions(
            title: 'ANCHOR_SURVIVOR_$index',
            size: const Size(460, 300),
          ),
        );
        children.add(id);
      }
      await Future<void>.delayed(const Duration(milliseconds: 350));
      report['childIds'] = children;
      report['beforeViewIds'] = _viewsSnapshot();
      await File('poc_anchor_before.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );

      // This callback is intentionally independent of the primary view state.
      // If all Flutter Views die, Dart will not reach it and no after-file
      // will be produced; the external runner records that as an observation.
      Future<void>.delayed(const Duration(milliseconds: 1600), () async {
        report['afterViewIds'] = _viewsSnapshot();
        report['nativeWindowIds'] = MultiViewDesktop.allWindowViewIds;
        report['remainingChildren'] = children
            .where(MultiViewDesktop.allWindowViewIds.contains)
            .toList();
        report['afterPrimaryCloseObserved'] = true;
        await File('poc_anchor_after.json').writeAsString(
          const JsonEncoder.withIndent('  ').convert(report),
          flush: true,
        );
        trace('ANCHOR_PROBE ${jsonEncode(report)}');
        await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
      });
      trace('ANCHOR_PROBE request primary close');
      await MultiViewDesktop.fromId(1).closeWindow();
    } catch (error, stack) {
      report['error'] = '$error\n$stack';
      await File('poc_anchor_error.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
    }
  }

  Future<void> _displayProbe() async {
    final report = <String, Object?>{
      'processId': pid,
      'scenario': 'dual_monitor_move_and_resize',
      'complete': false,
    };
    try {
      final firstFrame = Completer<Map<String, Object?>>();
      final id = await openWindow(
        (_, viewId) => _ChildView(
          index: 995,
          id: viewId,
          onFirstFrame: (dpr, size) {
            if (!firstFrame.isCompleted) {
              firstFrame.complete({
                'devicePixelRatio': dpr,
                'physicalSize': [size.width, size.height],
              });
            }
          },
        ),
        options: const WindowOptions(
          title: 'MVD_MOVE_AND_RESIZE',
          size: Size(620, 420),
        ),
      );
      final child = MultiViewDesktop.fromId(id);
      report['childViewId'] = id;
      report['firstFrame'] = await firstFrame.future.timeout(
        const Duration(seconds: 6),
      );
      report['initialPosition'] = [
        child.getPosition().dx,
        child.getPosition().dy,
      ];
      report['initialViews'] = _viewsSnapshot();

      await child.setPosition(const Offset(-1200, 220));
      await Future<void>.delayed(const Duration(milliseconds: 550));
      report['secondaryPosition'] = [
        child.getPosition().dx,
        child.getPosition().dy,
      ];
      report['secondaryViews'] = _viewsSnapshot();

      report['resizeAccepted'] = await child.setSize(const Size(820, 580));
      await Future<void>.delayed(const Duration(milliseconds: 350));
      report['resizedViews'] = _viewsSnapshot();

      await child.setPosition(const Offset(700, 240));
      await Future<void>.delayed(const Duration(milliseconds: 550));
      report['primaryPosition'] = [
        child.getPosition().dx,
        child.getPosition().dy,
      ];
      report['primaryViews'] = _viewsSnapshot();

      await child.closeWindow();
      await Future<void>.delayed(const Duration(milliseconds: 200));
      report['viewsAfterClose'] = _viewsSnapshot();
      report['complete'] = true;
    } catch (error, stack) {
      report['error'] = '$error\n$stack';
    } finally {
      await File('poc_display_probe.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
      trace('DISPLAY_PROBE ${jsonEncode(report)}');
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
    }
  }

  Future<void> _pluginProbe() async {
    final report = <String, Object?>{
      'processId': pid,
      'scenario': 'window_manager_0.5.2_called_from_child',
      'complete': false,
    };
    try {
      await wm.windowManager.ensureInitialized();
      final anchor = MultiViewDesktop.fromId(1);
      anchor.setTitle('MVD_MAIN_BEFORE_CHILD');
      final childCalled = Completer<Map<String, Object?>>();
      final childId = await openWindow(
        (_, viewId) => _ChildView(
          index: 999,
          id: viewId,
          onFirstFrame: (_, _) {
            unawaited(() async {
              try {
                final observedId = await wm.windowManager.getId();
                await wm.windowManager.setTitle('WM_CHANGED_FROM_CHILD');
                childCalled.complete({
                  'windowManagerGetIdFromChild': observedId,
                  'callResult': 'ok',
                });
              } catch (error) {
                childCalled.complete({'callResult': 'error: $error'});
              }
            }());
          },
        ),
        options: const WindowOptions(
          title: 'MVD_CHILD_BEFORE_CALL',
          size: Size(510, 340),
        ),
      );
      report['childViewId'] = childId;
      report.addAll(
        await childCalled.future.timeout(const Duration(seconds: 8)),
      );
      await Future<void>.delayed(const Duration(milliseconds: 230));
      final parentTitle = anchor.getTitle();
      final child = MultiViewDesktop.fromId(childId);
      final childTitle = child.getTitle();
      report['mainTitleAfterWindowManagerFromChild'] = parentTitle;
      report['childTitleAfterWindowManagerFromChild'] = childTitle;
      report['windowManagerTargetsChild'] =
          childTitle == 'WM_CHANGED_FROM_CHILD';
      report['windowManagerChangesMain'] =
          parentTitle == 'WM_CHANGED_FROM_CHILD';
      // Control: the multiview API explicitly addresses the selected child.
      child.setTitle('MVD_EXPLICIT_CHILD');
      report['mvdExplicitChildRoutingWorks'] =
          child.getTitle() == 'MVD_EXPLICIT_CHILD' &&
          anchor.getTitle() == parentTitle;
      await child.closeWindow();
      await Future<void>.delayed(const Duration(milliseconds: 130));
      report['viewsAfterChildClose'] =
          WidgetsBinding.instance.platformDispatcher.views.length;
      report['complete'] = true;
    } catch (error, stack) {
      report['error'] = '$error\n$stack';
    } finally {
      await File('poc_window_manager_probe.json').writeAsString(
        const JsonEncoder.withIndent('  ').convert(report),
        flush: true,
      );
      trace('WINDOW_MANAGER_PROBE ${jsonEncode(report)}');
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await MultiViewDesktop.closeApp(closeMode: CloseMode.destroy);
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Alera: Single Engine / Many Views')),
    body: Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('PID $pid — $status'),
          ValueListenableBuilder<int>(
            valueListenable: sharedCounter,
            builder: (_, count, _) => Text('Shared state: $count'),
          ),
          const SizedBox(height: 18),
          FilledButton(
            onPressed: _smoke,
            child: const Text('Run 10-window lifecycle test'),
          ),
          OutlinedButton(
            onPressed: () => unawaited(
              openWindow(
                (_, id) => _ChildView(index: 99, id: id),
                options: WindowOptions(title: 'Manual child'),
              ),
            ),
            child: const Text('Open child window'),
          ),
        ],
      ),
    ),
  );
}

class _ChildView extends StatefulWidget {
  const _ChildView({required this.index, required this.id, this.onFirstFrame});
  final int index;
  final int id;
  final void Function(double devicePixelRatio, Size physicalSize)? onFirstFrame;
  @override
  State<_ChildView> createState() => _ChildViewState();
}

class _ChildViewState extends State<_ChildView> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      sharedCounter.value++;
      widget.onFirstFrame?.call(
        View.of(context).devicePixelRatio,
        View.of(context).physicalSize,
      );
      trace('DART child-view first frame id=${widget.id}');
    });
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: Text('Window ${widget.index}')),
    body: Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('Same PID: $pid, view ID: ${widget.id}'),
          if (widget.index == 910 || widget.index == 911)
            SizedBox(
              width: 300,
              child: TextField(
                autofocus: true,
                onChanged: (value) {
                  inputValuesByViewId[widget.id] = value;
                  trace(
                    'CHILD_INPUT id=${widget.id} index=${widget.index} value=$value',
                  );
                },
                decoration: InputDecoration(
                  labelText: 'Input window ${widget.index}',
                ),
              ),
            ),
          ValueListenableBuilder<int>(
            valueListenable: sharedCounter,
            builder: (_, count, _) => Text('Shared counter: $count'),
          ),
          TextButton(
            onPressed: () =>
                unawaited(MultiViewDesktop.of(context).closeWindow()),
            child: const Text('Close this view'),
          ),
        ],
      ),
    ),
  );
}
