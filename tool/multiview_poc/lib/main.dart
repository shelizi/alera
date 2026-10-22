import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:multiview_desktop/multiview_desktop.dart';

void trace(String message) {
  File('poc_startup_trace.txt').writeAsStringSync(
    '${DateTime.now().toIso8601String()} pid=$pid $message\n',
    mode: FileMode.append,
    flush: true,
  );
}

// Shared instance: when opening child windows there is no Dart serialization.
final sharedCounter = ValueNotifier<int>(0);

void main(List<String> args) {
  trace('DART main args=$args');
  FlutterError.onError = (details) {
    trace('FLUTTER_ERROR ${details.exceptionAsString()}');
  };
  runMultiApp(
    home: (_, _) => MaterialApp(
      title: 'Alera single-engine PoC',
      theme: ThemeData.dark(useMaterial3: true),
      home: _MainView(
        autoSmoke: args.contains('--poc-smoke'),
        cycleCount: args.contains('--poc-stress') ? 50 : 10,
      ),
    ),
    config: MultiAppConfig(
      fileLogParams: const LogParams(enable: true),
      generalParams: MultiPlatformParams(
        closeMode: CloseMode.softCascade,
        closeAppAfterLastWindowClosed: true,
        animation: ViewAnimationConfig.disabled,
      ),
    ),
  );
}

class _MainView extends StatefulWidget {
  const _MainView({required this.autoSmoke, required this.cycleCount});
  final bool autoSmoke;
  final int cycleCount;
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
        final id = await openWindow(
          (_, viewId) => _ChildView(index: index, id: viewId),
          options: WindowOptions(
            title: 'Alera PoC $index',
            size: const Size(480, 320),
          ),
        );
        final createMs = startedAt.elapsedMilliseconds;
        await Future<void>.delayed(const Duration(milliseconds: 200));
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
        await Future<void>.delayed(const Duration(milliseconds: 100));
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
  const _ChildView({required this.index, required this.id});
  final int index;
  final int id;
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
