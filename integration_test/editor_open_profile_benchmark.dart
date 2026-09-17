/// C4 editor native-open handoff profiler.
///
/// Run on the same Windows desktop used for the existing large-file benchmark:
///   flutter test integration_test/editor_open_profile_benchmark.dart -d windows
///
/// This is a measurement harness, not a timing assertion. It deliberately keeps
/// production CodeForge/Alera sources untouched while decomposing the 50k-line
/// open path into file/native/FFI, controller/Rope, viewport, syntax-control,
/// and first-frame stages.
library;

import 'dart:convert';
import 'dart:developer' as developer;
import 'dart:io';
import 'dart:math';

import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/rust/frb_generated.dart' as alera_rust;
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:code_forge/code_forge/syntax_highlighter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path/path.dart' as p;
import 'package:re_highlight/languages/all.dart';

const _sampleCount = 5;
const _lineCount = 50000;
const _syntaxControlLines = 40;
const _viewportLines = 80;

String _largeDocument() {
  final out = StringBuffer();
  for (var i = 0; i < _lineCount; i++) {
    out
      ..write('final value_')
      ..write(i.toString().padLeft(6, '0'))
      ..write(" = 'needle123 alpha beta gamma delta epsilon ")
      ..write(i % 97)
      ..write("'; // benchmark\n");
  }
  return out.toString();
}

class _Stats {
  const _Stats(this.samplesMicros);

  final List<int> samplesMicros;

  static double _percentile(List<double> values, double fraction) {
    if (values.isEmpty) return 0;
    final sorted = [...values]..sort();
    return sorted[((sorted.length - 1) * fraction).round()];
  }

  List<double> get milliseconds =>
      samplesMicros.map((value) => value / 1000.0).toList(growable: false);

  double get medianMs => _percentile(milliseconds, 0.5);
  double get p95Ms => _percentile(milliseconds, 0.95);

  double get madMs {
    final values = milliseconds;
    if (values.isEmpty) return 0;
    final median = medianMs;
    return _percentile(
      values.map((value) => (value - median).abs()).toList(growable: false),
      0.5,
    );
  }

  @override
  String toString() =>
      'median=${medianMs.toStringAsFixed(2)} ms '
      'p95=${p95Ms.toStringAsFixed(2)} ms '
      'mad=${madMs.toStringAsFixed(2)} ms '
      'samples=${samplesMicros.map((value) => (value / 1000).toStringAsFixed(2)).join(',')}';
}

class _StageReport {
  const _StageReport(this.name, this.stats, {this.note});

  final String name;
  final _Stats stats;
  final String? note;

  @override
  String toString() => '$name: $stats${note == null ? '' : ' [$note]'}';
}

class _FrameStageReport {
  const _FrameStageReport({
    required this.wall,
    required this.build,
    required this.raster,
    required this.framesPerSample,
  });

  final _StageReport wall;
  final _Stats build;
  final _Stats raster;
  final List<int> framesPerSample;

  @override
  String toString() =>
      '${wall.toString()}\n'
      '  build: $build\n'
      '  raster: $raster\n'
      '  frames_per_sample=${framesPerSample.join(',')}';
}

Future<_StageReport> _measureAsync(
  String name,
  Future<void> Function() action, {
  Future<void> Function()? warmUp,
  String? note,
}) async {
  await (warmUp ?? action)();
  final samples = <int>[];
  for (var i = 0; i < _sampleCount; i++) {
    developer.Timeline.startSync('EditorOpenProfile.$name');
    final watch = Stopwatch()..start();
    try {
      await action();
    } finally {
      watch.stop();
      developer.Timeline.finishSync();
    }
    samples.add(watch.elapsedMicroseconds);
  }
  return _StageReport(name, _Stats(samples), note: note);
}

_StageReport _measureSync(
  String name,
  void Function() action, {
  void Function()? warmUp,
  String? note,
}) {
  (warmUp ?? action)();
  final samples = <int>[];
  for (var i = 0; i < _sampleCount; i++) {
    developer.Timeline.startSync('EditorOpenProfile.$name');
    final watch = Stopwatch()..start();
    try {
      action();
    } finally {
      watch.stop();
      developer.Timeline.finishSync();
    }
    samples.add(watch.elapsedMicroseconds);
  }
  return _StageReport(name, _Stats(samples), note: note);
}

Widget _editorWidget(code_forge.CodeForgeController controller) => MaterialApp(
  debugShowCheckedModeBanner: false,
  home: Scaffold(
    body: code_forge.CodeForge(
      controller: controller,
      autoFocus: false,
      lineWrap: false,
      enableFolding: false,
      enableGuideLines: false,
      enableLocalSuggestions: false,
      largeFilePerformanceMode: true,
      language: builtinAllLanguages['dart']!,
      languageId: 'dart',
      textStyle: const TextStyle(
        fontFamily: 'JetBrains Mono',
        fontSize: 14,
        height: 1.35,
      ),
    ),
  ),
);

Future<
  ({
    code_forge.CodeForgeController controller,
    code_forge.WorkspaceSourceInfo sourceInfo,
  })
>
_openNativeController({
  required String workspacePath,
  required String relativePath,
}) async {
  final controller = code_forge.CodeForgeController();
  final sourceInfo = await controller.openWorkspaceFile(
    workspacePath: workspacePath,
    relativePath: relativePath,
    tabSize: 2,
  );
  expect(controller.lineCount, greaterThanOrEqualTo(_lineCount));
  return (controller: controller, sourceInfo: sourceInfo);
}

Future<_FrameStageReport> _measureFirstFrame(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding, {
  required String workspacePath,
  required String relativePath,
}) async {
  Future<void> warmUp() async {
    final opened = await _openNativeController(
      workspacePath: workspacePath,
      relativePath: relativePath,
    );
    await tester.pumpWidget(_editorWidget(opened.controller));
    await tester.pumpWidget(const SizedBox.shrink());
    opened.controller.dispose();
  }

  await warmUp();
  final wallSamples = <int>[];
  final buildSamples = <int>[];
  final rasterSamples = <int>[];
  final framesPerSample = <int>[];

  for (var i = 0; i < _sampleCount; i++) {
    final opened = await _openNativeController(
      workspacePath: workspacePath,
      relativePath: relativePath,
    );
    final controller = opened.controller;
    final timings = <FrameTiming>[];
    void collect(List<FrameTiming> frames) => timings.addAll(frames);
    binding.addTimingsCallback(collect);

    developer.Timeline.startSync('EditorOpenProfile.first_codeforge_frame');
    final watch = Stopwatch()..start();
    await tester.pumpWidget(_editorWidget(controller));
    watch.stop();
    developer.Timeline.finishSync();
    wallSamples.add(watch.elapsedMicroseconds);

    await Future<void>.delayed(const Duration(milliseconds: 40));
    binding.removeTimingsCallback(collect);
    framesPerSample.add(timings.length);
    if (timings.isNotEmpty) {
      buildSamples.add(
        timings.map((frame) => frame.buildDuration.inMicroseconds).reduce(max),
      );
      rasterSamples.add(
        timings.map((frame) => frame.rasterDuration.inMicroseconds).reduce(max),
      );
    }

    await tester.pumpWidget(const SizedBox.shrink());
    controller.dispose();
  }

  return _FrameStageReport(
    wall: _StageReport(
      'first_codeforge_frame',
      _Stats(wallSamples),
      note: 'C4 native Rope already opened; retained syntax setup may start asynchronously during widget construction',
    ),
    build: _Stats(buildSamples),
    raster: _Stats(rasterSamples),
    framesPerSample: framesPerSample,
  );
}

Future<_FrameStageReport> _measureSameControllerRebuild(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding, {
  required String workspacePath,
  required String relativePath,
}) async {
  final opened = await _openNativeController(
    workspacePath: workspacePath,
    relativePath: relativePath,
  );
  final controller = opened.controller;
  await tester.pumpWidget(_editorWidget(controller));
  await Future<void>.delayed(const Duration(milliseconds: 40));

  final wallSamples = <int>[];
  final buildSamples = <int>[];
  final rasterSamples = <int>[];
  final framesPerSample = <int>[];

  for (var i = 0; i < _sampleCount; i++) {
    final timings = <FrameTiming>[];
    void collect(List<FrameTiming> frames) => timings.addAll(frames);
    binding.addTimingsCallback(collect);

    developer.Timeline.startSync('EditorOpenProfile.same_controller_rebuild');
    final watch = Stopwatch()..start();
    await tester.pumpWidget(_editorWidget(controller));
    watch.stop();
    developer.Timeline.finishSync();
    wallSamples.add(watch.elapsedMicroseconds);

    await Future<void>.delayed(const Duration(milliseconds: 40));
    binding.removeTimingsCallback(collect);
    framesPerSample.add(timings.length);
    if (timings.isNotEmpty) {
      buildSamples.add(
        timings.map((frame) => frame.buildDuration.inMicroseconds).reduce(max),
      );
      rasterSamples.add(
        timings.map((frame) => frame.rasterDuration.inMicroseconds).reduce(max),
      );
    }
  }

  await tester.pumpWidget(const SizedBox.shrink());
  controller.dispose();

  return _FrameStageReport(
    wall: _StageReport(
      'same_controller_rebuild',
      _Stats(wallSamples),
      note: 'diagnostic widget/provider fan-out proxy; existing native Rope retained',
    ),
    build: _Stats(buildSamples),
    raster: _Stats(rasterSamples),
    framesPerSample: framesPerSample,
  );
}

Future<_FrameStageReport> _measureFullOpenToFirstFrame(
  WidgetTester tester,
  IntegrationTestWidgetsFlutterBinding binding, {
  required String workspacePath,
  required String relativePath,
}) async {
  Future<void> runOnce({
    required bool record,
    List<int>? walls,
    List<int>? builds,
    List<int>? rasters,
    List<int>? frameCounts,
  }) async {
    final timings = <FrameTiming>[];
    void collect(List<FrameTiming> frames) => timings.addAll(frames);
    if (record) binding.addTimingsCallback(collect);

    developer.Timeline.startSync('EditorOpenProfile.full_open_to_first_frame');
    final watch = Stopwatch()..start();
    final opened = await _openNativeController(
      workspacePath: workspacePath,
      relativePath: relativePath,
    );
    final controller = opened.controller;
    final find = code_forge.FindController(controller);
    await tester.pumpWidget(_editorWidget(controller));
    watch.stop();
    developer.Timeline.finishSync();

    if (record) {
      walls!.add(watch.elapsedMicroseconds);
      await Future<void>.delayed(const Duration(milliseconds: 40));
      binding.removeTimingsCallback(collect);
      frameCounts!.add(timings.length);
      if (timings.isNotEmpty) {
        builds!.add(
          timings
              .map((frame) => frame.buildDuration.inMicroseconds)
              .reduce(max),
        );
        rasters!.add(
          timings
              .map((frame) => frame.rasterDuration.inMicroseconds)
              .reduce(max),
        );
      }
    }

    await tester.pumpWidget(const SizedBox.shrink());
    find.dispose();
    controller.dispose();
  }

  await runOnce(record: false);
  final walls = <int>[];
  final builds = <int>[];
  final rasters = <int>[];
  final frameCounts = <int>[];
  for (var i = 0; i < _sampleCount; i++) {
    await runOnce(
      record: true,
      walls: walls,
      builds: builds,
      rasters: rasters,
      frameCounts: frameCounts,
    );
  }
  return _FrameStageReport(
    wall: _StageReport(
      'full_open_to_first_frame',
      _Stats(walls),
      note: 'C4 direct native file open + native Rope + FindController + first CodeForge frame; no whole-document Dart String',
    ),
    build: _Stats(builds),
    raster: _Stats(rasters),
    framesPerSample: frameCounts,
  );
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('profiles C4 native-open handoff stages', (tester) async {
    final large = _largeDocument();
    final workspace = await Directory.systemTemp.createTemp(
      'alera_editor_open_profile_',
    );
    addTearDown(() => workspace.delete(recursive: true));
    const relativePath = 'profile_fixture.dart';
    final fixture = File(p.join(workspace.path, relativePath));
    await fixture.writeAsString(large, flush: true);
    final fixtureBytes = await fixture.readAsBytes();

    final aleraInit = Stopwatch()..start();
    await alera_rust.RustLib.init();
    aleraInit.stop();
    final codeForgeInit = Stopwatch()..start();
    await code_forge.RustLib.init();
    codeForgeInit.stop();

    final reports = <_StageReport>[];

    List<int> rawBytes = const [];
    reports.add(
      await _measureAsync('raw_file_read', () async {
        rawBytes = await fixture.readAsBytes();
        expect(rawBytes.length, fixtureBytes.length);
      }, note: 'diagnostic subset; warm OS-cache read'),
    );

    String decoded = '';
    reports.add(
      _measureSync('dart_utf8_decode', () {
        decoded = utf8.decode(fixtureBytes);
        expect(decoded.length, large.length);
      }, note: 'legacy diagnostic control; Dart decode only'),
    );

    native.WorkspaceDecodedText? nativeDecoded;
    reports.add(
      await _measureAsync(
        'legacy_native_decode_ffi',
        () async {
          nativeDecoded = await native.decodeWorkspaceTextBytes(
            bytes: fixtureBytes,
          );
          expect(nativeDecoded!.content.length, large.length);
        },
        note: 'legacy control; Dart bytes -> Alera Rust decode -> Dart String',
      ),
    );

    native.WorkspaceEditorTextFile? legacyEditorFile;
    reports.add(
      await _measureAsync(
        'legacy_alera_editor_read',
        () async {
          legacyEditorFile = await native.readWorkspaceEditorTextFile(
            workspacePath: workspace.path,
            relativePath: relativePath,
            tabSize: 2,
          );
          expect(legacyEditorFile!.displayContent.length, large.length);
        },
        note: 'legacy C3 control; Alera read/decode/normalize then whole Strings cross FRB',
      ),
    );

    reports.add(
      _measureSync('controller_init', () {
        final controller = code_forge.CodeForgeController();
        final find = code_forge.FindController(controller);
        find.dispose();
        controller.dispose();
      }, note: 'diagnostic controller + FindController construction'),
    );

    code_forge.WorkspaceSourceInfo? nativeSourceInfo;
    reports.add(
      await _measureAsync(
        'codeforge_native_workspace_open',
        () async {
          final opened = await _openNativeController(
            workspacePath: workspace.path,
            relativePath: relativePath,
          );
          nativeSourceInfo = opened.sourceInfo;
          expect(nativeSourceInfo!.displayChars.toInt(), large.length);
          opened.controller.dispose();
        },
        note: 'C4 production handoff: file read/decode/tab normalization -> native Rope; Dart receives bounded metadata only',
      ),
    );

    reports.add(
      _measureSync(
        'legacy_string_to_rope_construction',
        () {
          final controller = code_forge.CodeForgeController();
          controller.text = legacyEditorFile!.displayContent;
          expect(controller.lineCount, greaterThanOrEqualTo(_lineCount));
          controller.dispose();
        },
        note: 'legacy C3 control only: full Dart String -> Rust RopeBridge.create',
      ),
    );

    reports.add(
      await _measureAsync(
        'native_open_plus_first_viewport',
        () async {
          final opened = await _openNativeController(
            workspacePath: workspace.path,
            relativePath: relativePath,
          );
          final lines = opened.controller.getLinesRange(0, _viewportLines);
          expect(lines, hasLength(_viewportLines));
          opened.controller.dispose();
        },
        note: 'C4 native open plus bounded viewport materialization; subtract codeforge_native_workspace_open for viewport-only intuition',
      ),
    );

    reports.add(
      await _measureAsync(
        'retained_native_syntax_first_viewport',
        () async {
          final opened = await _openNativeController(
            workspacePath: workspace.path,
            relativePath: relativePath,
          );
          opened.controller.configureNativeSyntaxDocument(
            languageId: 'dart',
            documentId: relativePath,
          );
          final spans = await opened.controller.queryNativeSyntaxSpans(
            startLine: 0,
            endLine: _viewportLines - 1,
            overscan: 0,
          );
          expect(spans, isNotNull);
          opened.controller.dispose();
        },
        note: 'C4 native open + structural Rope clone + retained Tree-sitter parse/query; subtract native open for native-document/syntax intuition',
      ),
    );

    final syntaxLines = large
        .split('\n')
        .take(_syntaxControlLines)
        .toList(growable: false);
    reports.add(
      await _measureAsync(
        'legacy_rehighlight_first_viewport_control',
        () async {
          final highlighter = SyntaxHighlighter(
            language: builtinAllLanguages['dart']!,
            languageId: 'dart',
            editorTheme: const <String, TextStyle>{
              'root': TextStyle(color: Colors.black),
            },
            baseTextStyle: const TextStyle(
              fontFamily: 'JetBrains Mono',
              fontSize: 14,
            ),
          );
          await highlighter.preHighlightLines(
            0,
            syntaxLines.length - 1,
            (line) => syntaxLines[line],
          );
        },
        note: 'legacy/fallback syntax control; not the C4 retained native-tree production path',
      ),
    );

    final firstFrame = await _measureFirstFrame(
      tester,
      binding,
      workspacePath: workspace.path,
      relativePath: relativePath,
    );
    final sameControllerRebuild = await _measureSameControllerRebuild(
      tester,
      binding,
      workspacePath: workspace.path,
      relativePath: relativePath,
    );
    final fullOpen = await _measureFullOpenToFirstFrame(
      tester,
      binding,
      workspacePath: workspace.path,
      relativePath: relativePath,
    );

    // ignore: avoid_print
    print('\n=== C4 editor native-open profile ===');
    // ignore: avoid_print
    print(
      'fixture_lines=$_lineCount fixture_bytes=${fixtureBytes.length} raw_chars=${nativeSourceInfo!.rawChars} display_chars=${nativeSourceInfo!.displayChars}',
    );
    // ignore: avoid_print
    print(
      'one_time_alera_rust_init_ms=${(aleraInit.elapsedMicroseconds / 1000).toStringAsFixed(2)} one_time_codeforge_rust_init_ms=${(codeForgeInit.elapsedMicroseconds / 1000).toStringAsFixed(2)}',
    );
    for (final report in reports) {
      // ignore: avoid_print
      print(report);
    }
    // ignore: avoid_print
    print(firstFrame);
    // ignore: avoid_print
    print(sameControllerRebuild);
    // ignore: avoid_print
    print(fullOpen);

    final c4Disjoint =
        <_StageReport>[
          reports.firstWhere(
            (report) => report.name == 'codeforge_native_workspace_open',
          ),
          firstFrame.wall,
        ]..sort(
          (left, right) => right.stats.medianMs.compareTo(left.stats.medianMs),
        );
    // ignore: avoid_print
    print(
      'c4_first_frame_contributors=${c4Disjoint.map((report) => '${report.name}:${report.stats.medianMs.toStringAsFixed(2)}ms').join(' > ')}',
    );

    expect(reports, hasLength(10));
    expect(firstFrame.wall.stats.samplesMicros, hasLength(_sampleCount));
    expect(
      sameControllerRebuild.wall.stats.samplesMicros,
      hasLength(_sampleCount),
    );
    expect(fullOpen.wall.stats.samplesMicros, hasLength(_sampleCount));
  }, timeout: const Timeout(Duration(minutes: 12)));
}
