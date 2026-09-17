/// Real-device large-file editor profiling gate for work package B.
///
/// Run with:
///   flutter test integration_test/editor_large_file_benchmark.dart -d windows
///
/// This is a measurement harness, not a performance-threshold CI test. It
/// records five samples for each required B scenario and emits Timeline ranges
/// named `EditorLargeBenchmark.*` for DevTools correlation with the existing
/// `CodeForge.largeFileParagraphLayout` shaping events.
///
/// Set `BENCH_VM_METRICS=1` to additionally collect VM-service UI CPU,
/// allocation, heap, and GC evidence. That mode has measurable observer effect,
/// so use the default mode for frame/jank comparisons.
library;

import 'dart:async';
import 'dart:developer' as developer;
import 'dart:io';
import 'dart:math';

import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:vm_service/utils.dart' as vm_utils;
import 'package:vm_service/vm_service.dart' as vm;
import 'package:vm_service/vm_service_io.dart';

const _sampleCount = 5;
const _lineCount = 50000;
const _longLineChars = 30000;
const _frameDelay = Duration(milliseconds: 16);
final _collectVmMetrics = Platform.environment['BENCH_VM_METRICS'] == '1';

String _largeDocument() {
  final out = StringBuffer();
  for (var i = 0; i < _lineCount; i++) {
    out
      ..write('line_')
      ..write(i.toString().padLeft(6, '0'))
      ..write(' alpha beta needle123 gamma delta epsilon ')
      ..write(i % 97)
      ..write('\n');
  }
  return out.toString();
}

String _longLineDocument(String large) =>
    '${List<String>.filled(_longLineChars, 'M').join()}\n$large';

Future<void> _frame(WidgetTester tester) async {
  await tester.pump();
  await Future<void>.delayed(_frameDelay);
}

class _Harness {
  _Harness(String text)
    : controller = code_forge.CodeForgeController(),
      vertical = ScrollController(),
      horizontal = ScrollController() {
    controller.text = text;
    finder = code_forge.FindController(controller);
  }

  final code_forge.CodeForgeController controller;
  late final code_forge.FindController finder;
  final ScrollController vertical;
  final ScrollController horizontal;

  Widget widget() => MaterialApp(
    debugShowCheckedModeBanner: false,
    home: Scaffold(
      body: code_forge.CodeForge(
        controller: controller,
        findController: finder,
        verticalScrollController: vertical,
        horizontalScrollController: horizontal,
        autoFocus: true,
        lineWrap: false,
        enableFolding: false,
        enableGuideLines: false,
        enableLocalSuggestions: false,
        largeFilePerformanceMode: true,
        textStyle: const TextStyle(
          fontFamily: 'JetBrains Mono',
          fontSize: 14,
          height: 1.35,
        ),
      ),
    ),
  );

  void dispose() {
    finder.dispose();
    controller.dispose();
    vertical.dispose();
    horizontal.dispose();
  }
}

class _VmSampleStart {
  const _VmSampleStart({required this.timelineMicros, required this.heapBytes});

  final int timelineMicros;
  final int heapBytes;
}

class _VmMetrics {
  _VmMetrics(this.service, this.isolateId);

  final vm.VmService service;
  final String isolateId;
  StreamSubscription<vm.Event>? _gcSubscription;
  int _gcEvents = 0;

  static Future<_VmMetrics?> connect() async {
    final serviceInfo = await developer.Service.getInfo();
    final serviceUri = serviceInfo.serverUri;
    if (serviceUri == null) return null;
    final websocketUri = vm_utils.convertToWebSocketUrl(
      serviceProtocolUrl: serviceUri,
    );
    final service = await vmServiceConnectUri(websocketUri.toString());
    final vmInfo = await service.getVM();
    final isolates = vmInfo.isolates ?? const <vm.IsolateRef>[];
    if (isolates.isEmpty) {
      await service.dispose();
      return null;
    }
    final isolate =
        isolates.where((item) => item.name == 'main').firstOrNull ??
        isolates.first;
    final isolateId = isolate.id;
    if (isolateId == null) {
      await service.dispose();
      return null;
    }
    final metrics = _VmMetrics(service, isolateId);
    try {
      await service.streamListen(vm.EventStreams.kGC);
      metrics._gcSubscription = service.onGCEvent.listen((event) {
        if (event.isolate?.id == isolateId) metrics._gcEvents++;
      });
    } on vm.RPCError catch (error) {
      // Another client may already have subscribed. Allocation/CPU metrics are
      // still useful; only natural GC event counts will be unavailable.
      if (error.code != vm.RPCErrorKind.kStreamAlreadySubscribed.code) rethrow;
    }
    return metrics;
  }

  Future<_VmSampleStart> begin() async {
    final profile = await service.getAllocationProfile(isolateId, reset: true);
    _gcEvents = 0;
    final timestamp = await service.getVMTimelineMicros();
    return _VmSampleStart(
      timelineMicros: timestamp.timestamp ?? 0,
      heapBytes: profile.memoryUsage?.heapUsage ?? 0,
    );
  }

  Future<
    ({
      double uiCpuMs,
      int allocatedBytes,
      int allocatedInstances,
      int heapDeltaBytes,
      int gcEvents,
    })
  >
  finish(_VmSampleStart start) async {
    final timestamp = await service.getVMTimelineMicros();
    final endMicros = timestamp.timestamp ?? start.timelineMicros;
    final extentMicros = max(1, endMicros - start.timelineMicros);
    final cpu = await service.getCpuSamples(
      isolateId,
      start.timelineMicros,
      extentMicros,
    );
    final profile = await service.getAllocationProfile(isolateId);
    var allocatedBytes = 0;
    var allocatedInstances = 0;
    for (final member in profile.members ?? const <vm.ClassHeapStats>[]) {
      allocatedBytes += max(0, member.accumulatedSize ?? 0);
      allocatedInstances += max(0, member.instancesAccumulated ?? 0);
    }
    final samplePeriodMicros = max(0, cpu.samplePeriod ?? 0);
    final uiCpuMs = (cpu.samples?.length ?? 0) * samplePeriodMicros / 1000.0;
    return (
      uiCpuMs: uiCpuMs,
      allocatedBytes: allocatedBytes,
      allocatedInstances: allocatedInstances,
      heapDeltaBytes:
          (profile.memoryUsage?.heapUsage ?? start.heapBytes) - start.heapBytes,
      gcEvents: _gcEvents,
    );
  }

  Future<void> dispose() async {
    await _gcSubscription?.cancel();
    await service.dispose();
  }
}

class _Sample {
  const _Sample({
    required this.wallMs,
    required this.rssDeltaBytes,
    required this.frames,
    required this.uiCpuMs,
    required this.allocatedBytes,
    required this.allocatedInstances,
    required this.heapDeltaBytes,
    required this.gcEvents,
  });

  final double wallMs;
  final int rssDeltaBytes;
  final List<FrameTiming> frames;
  final double? uiCpuMs;
  final int? allocatedBytes;
  final int? allocatedInstances;
  final int? heapDeltaBytes;
  final int? gcEvents;
}

class _Report {
  const _Report(this.name, this.samples);
  final String name;
  final List<_Sample> samples;

  static double percentile(List<double> values, double fraction) {
    if (values.isEmpty) return 0;
    final sorted = [...values]..sort();
    return sorted[((sorted.length - 1) * fraction).round()];
  }

  static double median(List<double> values) => percentile(values, 0.5);

  static double mad(List<double> values) {
    if (values.isEmpty) return 0;
    final m = median(values);
    return median(values.map((v) => (v - m).abs()).toList());
  }

  static String stats(List<double> values) =>
      'median=${median(values).toStringAsFixed(2)} '
      'p95=${percentile(values, 0.95).toStringAsFixed(2)} '
      'mad=${mad(values).toStringAsFixed(2)}';

  @override
  String toString() {
    final wall = samples.map((s) => s.wallMs).toList();
    final rss = samples.map((s) => s.rssDeltaBytes / (1024 * 1024)).toList();
    final uiCpu = samples
        .where((s) => s.uiCpuMs != null)
        .map((s) => s.uiCpuMs!)
        .toList();
    final allocatedMb = samples
        .where((s) => s.allocatedBytes != null)
        .map((s) => s.allocatedBytes! / (1024 * 1024))
        .toList();
    final allocatedInstances = samples
        .where((s) => s.allocatedInstances != null)
        .map((s) => s.allocatedInstances!.toDouble())
        .toList();
    final heapDeltaMb = samples
        .where((s) => s.heapDeltaBytes != null)
        .map((s) => s.heapDeltaBytes! / (1024 * 1024))
        .toList();
    final gcEvents = samples
        .where((s) => s.gcEvents != null)
        .map((s) => s.gcEvents!.toDouble())
        .toList();
    final frames = <FrameTiming>[for (final s in samples) ...s.frames];
    final build = frames
        .map((f) => f.buildDuration.inMicroseconds / 1000)
        .toList();
    final raster = frames
        .map((f) => f.rasterDuration.inMicroseconds / 1000)
        .toList();
    final total = frames.map((f) => f.totalSpan.inMicroseconds / 1000).toList();
    final jank60 = total.where((ms) => ms > 16.7).length;
    final jank120 = total.where((ms) => ms > 8.3).length;
    return '$name\n'
        '  wall_ms ${stats(wall)} samples=${samples.length}\n'
        '  build_ms ${stats(build)} frames=${frames.length}\n'
        '  raster_ms ${stats(raster)}\n'
        '  frame_ms ${stats(total)} jank60=$jank60 jank120=$jank120\n'
        '  ui_cpu_ms ${uiCpu.isEmpty ? 'unavailable' : stats(uiCpu)}\n'
        '  allocated_mb ${allocatedMb.isEmpty ? 'unavailable' : stats(allocatedMb)}\n'
        '  allocated_instances ${allocatedInstances.isEmpty ? 'unavailable' : stats(allocatedInstances)}\n'
        '  heap_delta_mb ${heapDeltaMb.isEmpty ? 'unavailable' : stats(heapDeltaMb)}\n'
        '  gc_events ${gcEvents.isEmpty ? 'unavailable' : stats(gcEvents)}\n'
        '  rss_delta_mb ${stats(rss)}';
  }
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  _VmMetrics? vmMetrics;
  setUpAll(() async {
    await code_forge.RustLib.init();
    vmMetrics = _collectVmMetrics ? await _VmMetrics.connect() : null;
  });
  tearDownAll(() async {
    await vmMetrics?.dispose();
  });
  final large = _largeDocument();
  final longLine = _longLineDocument(large);
  final diagnostics = List<code_forge.LspErrors>.generate(10000, (i) {
    final line = (i * 5) % _lineCount;
    return code_forge.LspErrors(
      severity: i.isEven ? 1 : 2,
      range: <String, dynamic>{
        'start': <String, int>{'line': line, 'character': 8},
        'end': <String, int>{'line': line, 'character': 20},
      },
      message: 'benchmark diagnostic $i',
    );
  }, growable: false);

  Future<_Sample> sample(
    WidgetTester tester,
    String name,
    Future<void> Function() action,
  ) async {
    final frames = <FrameTiming>[];
    void collect(List<FrameTiming> values) => frames.addAll(values);
    final rssBefore = ProcessInfo.currentRss;
    final vmStart = await vmMetrics?.begin();
    binding.addTimingsCallback(collect);
    final watch = Stopwatch()..start();
    developer.Timeline.startSync('EditorLargeBenchmark.$name');
    try {
      await action();
    } finally {
      developer.Timeline.finishSync();
      watch.stop();
      await tester.pump();
      await Future<void>.delayed(const Duration(milliseconds: 40));
      binding.removeTimingsCallback(collect);
    }
    final vmResult = vmStart == null ? null : await vmMetrics?.finish(vmStart);
    return _Sample(
      wallMs: watch.elapsedMicroseconds / 1000,
      rssDeltaBytes: ProcessInfo.currentRss - rssBefore,
      frames: List<FrameTiming>.unmodifiable(frames),
      uiCpuMs: vmResult?.uiCpuMs,
      allocatedBytes: vmResult?.allocatedBytes,
      allocatedInstances: vmResult?.allocatedInstances,
      heapDeltaBytes: vmResult?.heapDeltaBytes,
      gcEvents: vmResult?.gcEvents,
    );
  }

  Future<_Report> measure(
    WidgetTester tester,
    String name, {
    required Future<void> Function() warmUp,
    required Future<void> Function() action,
    Future<void> Function()? reset,
  }) async {
    await warmUp();
    final samples = <_Sample>[];
    for (var i = 0; i < _sampleCount; i++) {
      samples.add(await sample(tester, name, action));
      if (reset != null) await reset();
    }
    final report = _Report(name, samples);
    // ignore: avoid_print
    print('\n$report');
    return report;
  }

  Future<void> mount(WidgetTester tester, _Harness harness) async {
    await tester.pumpWidget(harness.widget());
    await tester.pump();
    await Future<void>.delayed(const Duration(milliseconds: 80));
  }

  Future<void> waitForMatches(
    WidgetTester tester,
    code_forge.FindController finder,
    int expected,
  ) async {
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    while (finder.matchCount != expected) {
      if (DateTime.now().isAfter(deadline)) {
        fail('search timed out: ${finder.matchCount}/$expected matches');
      }
      await tester.pump(const Duration(milliseconds: 20));
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }
    await _frame(tester);
  }

  testWidgets('profiles current large-file editor hot paths', (tester) async {
    final reports = <_Report>[];

    await tester.pumpWidget(const SizedBox.shrink());
    Future<void> openOnce() async {
      final h = _Harness(large);
      await mount(tester, h);
      await tester.pumpWidget(const SizedBox.shrink());
      h.dispose();
    }

    reports.add(
      await measure(tester, 'open_reopen', warmUp: openOnce, action: openOnce),
    );

    var harness = _Harness(longLine);
    await mount(tester, harness);

    reports.add(
      await measure(
        tester,
        'fast_vertical_scroll',
        warmUp: () async {
          harness.vertical.jumpTo(
            harness.vertical.position.maxScrollExtent * 0.5,
          );
          await _frame(tester);
          harness.vertical.jumpTo(0);
          await _frame(tester);
        },
        action: () async {
          final maxScroll = harness.vertical.position.maxScrollExtent;
          for (var i = 0; i < 14; i++) {
            final raw = i.isEven ? (i + 1) / 15 : 1 - ((i + 1) / 15);
            harness.vertical.jumpTo(maxScroll * raw.clamp(0.0, 1.0));
            await _frame(tester);
          }
        },
      ),
    );

    reports.add(
      await measure(
        tester,
        'fast_horizontal_scroll_long_line',
        warmUp: () async {
          harness.horizontal.jumpTo(
            harness.horizontal.position.maxScrollExtent,
          );
          await _frame(tester);
          harness.horizontal.jumpTo(0);
          await _frame(tester);
        },
        action: () async {
          final maxScroll = harness.horizontal.position.maxScrollExtent;
          for (var i = 0; i < 14; i++) {
            final raw = i.isEven ? (i + 1) / 15 : 1 - ((i + 1) / 15);
            harness.horizontal.jumpTo(maxScroll * raw.clamp(0.0, 1.0));
            await _frame(tester);
          }
        },
      ),
    );

    final middle = harness.controller.getLineStartOffset(_lineCount ~/ 2);
    reports.add(
      await measure(
        tester,
        'caret_movement',
        warmUp: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          harness.controller.pressRightArrowKey();
          await _frame(tester);
        },
        action: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          for (var i = 0; i < 120; i++) {
            harness.controller.pressRightArrowKey();
            if (i % 12 == 11) await _frame(tester);
          }
        },
      ),
    );

    reports.add(
      await measure(
        tester,
        'word_movement',
        warmUp: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
          await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
          await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
          await _frame(tester);
        },
        action: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
          for (var i = 0; i < 60; i++) {
            await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
            if (i % 6 == 5) await _frame(tester);
          }
          await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        },
      ),
    );

    reports.add(
      await measure(
        tester,
        'selection_dragging',
        warmUp: () async {
          final box = tester.getRect(find.byType(code_forge.CodeForge));
          final g = await tester.startGesture(
            Offset(box.left + 140, box.top + 120),
          );
          await g.moveTo(Offset(box.left + 420, box.top + 240));
          await g.up();
          await _frame(tester);
        },
        action: () async {
          final box = tester.getRect(find.byType(code_forge.CodeForge));
          final start = Offset(box.left + 140, box.top + 120);
          final g = await tester.startGesture(start);
          for (var i = 1; i <= 12; i++) {
            await g.moveTo(start + Offset(i * 28.0, min(220.0, i * 18.0)));
            await _frame(tester);
          }
          await g.up();
        },
      ),
    );

    const burst = 'abcdefghij';
    reports.add(
      await measure(
        tester,
        'typing_burst',
        warmUp: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          harness.controller.insertAtCurrentCursor('x');
          await _frame(tester);
          harness.controller.replaceRange(middle, middle + 1, '');
        },
        action: () async {
          harness.controller.setSelectionSilently(
            TextSelection.collapsed(offset: middle),
          );
          for (var i = 0; i < 8; i++) {
            harness.controller.insertAtCurrentCursor(burst);
            await _frame(tester);
          }
        },
        reset: () async {
          harness.controller.replaceRange(
            middle,
            middle + burst.length * 8,
            '',
          );
          await _frame(tester);
        },
      ),
    );

    await tester.pumpWidget(const SizedBox.shrink());
    harness.dispose();
    harness = _Harness(large);
    await mount(tester, harness);

    reports.add(
      await measure(
        tester,
        'literal_search_many_matches',
        warmUp: () async {
          harness.finder.clear();
          harness.finder.find('needle123', scrollToMatch: false);
          await waitForMatches(tester, harness.finder, _lineCount);
        },
        action: () async {
          harness.finder.clear();
          harness.finder.find('needle123', scrollToMatch: false);
          await waitForMatches(tester, harness.finder, _lineCount);
        },
      ),
    );

    harness.finder.isRegex = true;
    reports.add(
      await measure(
        tester,
        'regex_search_many_matches',
        warmUp: () async {
          harness.finder.clear();
          harness.finder.find(r'needle\d+', scrollToMatch: false);
          await waitForMatches(tester, harness.finder, _lineCount);
        },
        action: () async {
          harness.finder.clear();
          harness.finder.find(r'needle\d+', scrollToMatch: false);
          await waitForMatches(tester, harness.finder, _lineCount);
        },
      ),
    );
    harness.finder.isRegex = false;
    harness.finder.clear();

    reports.add(
      await measure(
        tester,
        'diagnostics_heavy_scroll',
        warmUp: () async {
          harness.controller.diagnosticsNotifier.value = diagnostics;
          await _frame(tester);
          harness.controller.diagnosticsNotifier.value = const [];
          await _frame(tester);
        },
        action: () async {
          harness.controller.diagnosticsNotifier.value = diagnostics;
          await _frame(tester);
          final maxScroll = harness.vertical.position.maxScrollExtent;
          for (var i = 0; i < 10; i++) {
            harness.vertical.jumpTo(maxScroll * ((i + 1) / 11));
            await _frame(tester);
          }
        },
        reset: () async {
          harness.controller.diagnosticsNotifier.value = const [];
          await _frame(tester);
        },
      ),
    );

    expect(reports, hasLength(10));
    for (final report in reports) {
      expect(report.samples, hasLength(_sampleCount));
    }

    await tester.pumpWidget(const SizedBox.shrink());
    harness.dispose();
  }, timeout: const Timeout(Duration(minutes: 12)));
}
