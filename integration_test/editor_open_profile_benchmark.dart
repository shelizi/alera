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
const _c6pLineCounts = <int>[2000, 20000, 50000, 100000];

class _C6pLanguage {
  const _C6pLanguage(this.id, this.extension);

  final String id;
  final String extension;
}

const _c6pLanguages = <_C6pLanguage>[
  _C6pLanguage('rust', 'rs'),
  _C6pLanguage('dart', 'dart'),
  _C6pLanguage('typescript', 'ts'),
];

var _aleraRustInitialized = false;
var _codeForgeRustInitialized = false;

Future<void> _ensureRustLibrariesInitialized() async {
  if (!_aleraRustInitialized) {
    await alera_rust.RustLib.init();
    _aleraRustInitialized = true;
  }
  if (!_codeForgeRustInitialized) {
    await code_forge.RustLib.init();
    _codeForgeRustInitialized = true;
  }
}

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

String _c6pDocument(String languageId, int lineCount) {
  final out = StringBuffer();
  for (var i = 0; i < lineCount; i++) {
    switch (languageId) {
      case 'rust':
        out.writeln(
          'fn item_$i() { let value_$i: usize = $i; println!("{{}}", value_$i); }',
        );
        break;
      case 'dart':
        out.writeln(
          "final value_$i = 'needle alpha beta gamma $i'; // benchmark",
        );
        break;
      case 'typescript':
        out.writeln(
          'const value_$i: string = `needle alpha beta gamma $i`; // benchmark',
        );
        break;
      default:
        throw ArgumentError.value(languageId, 'languageId');
    }
  }
  return out.toString();
}

Map<String, Object> _statsPayload(List<int> samplesMicros) {
  final stats = _Stats(samplesMicros);
  return <String, Object>{
    'median_ms': stats.medianMs,
    'p95_ms': stats.p95Ms,
    'mad_ms': stats.madMs,
    'samples_us': samplesMicros,
  };
}

int _medianInt(List<int> values) {
  if (values.isEmpty) return 0;
  final sorted = [...values]..sort();
  return sorted[(sorted.length - 1) ~/ 2];
}

int _sourceInfoPayloadProxyBytes(code_forge.WorkspaceSourceInfo info) {
  final payload = <String, Object>{
    'encoding': info.encoding.name,
    'contentToken': info.contentToken,
    'modifiedMillis': info.modifiedMillis,
    'size': info.size.toString(),
    'rawChars': info.rawChars.toString(),
    'displayChars': info.displayChars.toString(),
  };
  return utf8.encode(jsonEncode(payload)).length;
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
      enableNativeSyntax: true,
      textStyle: const TextStyle(
        fontFamily: 'JetBrains Mono',
        fontSize: 14,
        height: 1.35,
      ),
    ),
  ),
);

Widget _c6pEditorWidget(
  code_forge.CodeForgeController controller,
  String languageId,
) => MaterialApp(
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
      language: builtinAllLanguages[languageId]!,
      languageId: languageId,
      enableNativeSyntax: true,
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
  int expectedMinimumLines = _lineCount,
}) async {
  final controller = code_forge.CodeForgeController();
  final sourceInfo = await controller.openWorkspaceFile(
    workspacePath: workspacePath,
    relativePath: relativePath,
    tabSize: 2,
  );
  expect(controller.lineCount, greaterThanOrEqualTo(expectedMinimumLines));
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

Future<Map<String, Object>> _measureC6pCase(
  WidgetTester tester, {
  required Directory workspace,
  required _C6pLanguage language,
  required int lineCount,
}) async {
  final relativePath = 'c6p_${language.id}_$lineCount.${language.extension}';
  final fixture = File(p.join(workspace.path, relativePath));
  await fixture.writeAsString(
    _c6pDocument(language.id, lineCount),
    flush: true,
  );

  final openSamples = <int>[];
  final firstFrameSamples = <int>[];
  final openToFirstFrameSamples = <int>[];
  final syntaxReadySamples = <int>[];
  final openToSyntaxReadySamples = <int>[];
  final rssOpenDeltas = <int>[];
  final rssSyntaxDeltas = <int>[];
  final payloadProxyBytes = <int>[];
  final spanCounts = <int>[];

  {
    final opened = await _openNativeController(
      workspacePath: workspace.path,
      relativePath: relativePath,
      expectedMinimumLines: lineCount,
    );
    await tester.pumpWidget(_c6pEditorWidget(opened.controller, language.id));
    await opened.controller.queryNativeSyntaxSpans(
      startLine: 0,
      endLine: _viewportLines - 1,
      overscan: 0,
    );
    await tester.pumpWidget(const SizedBox.shrink());
    opened.controller.dispose();
  }

  for (var sample = 0; sample < _sampleCount; sample++) {
    final rssBefore = ProcessInfo.currentRss;
    final totalWatch = Stopwatch()..start();
    final openWatch = Stopwatch()..start();
    final opened = await _openNativeController(
      workspacePath: workspace.path,
      relativePath: relativePath,
      expectedMinimumLines: lineCount,
    );
    openWatch.stop();
    openSamples.add(openWatch.elapsedMicroseconds);
    rssOpenDeltas.add(ProcessInfo.currentRss - rssBefore);
    payloadProxyBytes.add(_sourceInfoPayloadProxyBytes(opened.sourceInfo));

    final frameWatch = Stopwatch()..start();
    await tester.pumpWidget(_c6pEditorWidget(opened.controller, language.id));
    frameWatch.stop();
    firstFrameSamples.add(frameWatch.elapsedMicroseconds);
    openToFirstFrameSamples.add(totalWatch.elapsedMicroseconds);

    final syntaxWatch = Stopwatch()..start();
    final spans = await opened.controller.queryNativeSyntaxSpans(
      startLine: 0,
      endLine: _viewportLines - 1,
      overscan: 0,
    );
    syntaxWatch.stop();
    totalWatch.stop();
    syntaxReadySamples.add(syntaxWatch.elapsedMicroseconds);
    openToSyntaxReadySamples.add(totalWatch.elapsedMicroseconds);
    expect(spans, isNotNull);
    spanCounts.add(spans!.spans.length);
    rssSyntaxDeltas.add(ProcessInfo.currentRss - rssBefore);

    await tester.pumpWidget(const SizedBox.shrink());
    opened.controller.dispose();
  }

  return <String, Object>{
    'language': language.id,
    'lines': lineCount,
    'fixture_bytes': await fixture.length(),
    'native_rope_ready': _statsPayload(openSamples),
    'first_codeforge_frame': _statsPayload(firstFrameSamples),
    'native_open_to_first_useful_frame': _statsPayload(openToFirstFrameSamples),
    'syntax_ready_after_first_frame': _statsPayload(syntaxReadySamples),
    'native_open_to_syntax_ready': _statsPayload(openToSyntaxReadySamples),
    'rss_open_delta_bytes_median': _medianInt(rssOpenDeltas),
    'rss_syntax_delta_bytes_median': _medianInt(rssSyntaxDeltas),
    'rss_open_delta_bytes_samples': rssOpenDeltas,
    'rss_syntax_delta_bytes_samples': rssSyntaxDeltas,
    'source_info_payload_proxy_bytes': payloadProxyBytes,
    'source_info_payload_proxy_bytes_max': payloadProxyBytes.reduce(max),
    'viewport_span_counts': spanCounts,
  };
}

Future<Map<String, Object>> _measureRapidReplacement(
  WidgetTester tester, {
  required Directory workspace,
}) async {
  const language = _C6pLanguage('rust', 'rs');
  const lineCount = 20000;
  const replacementCount = 3;
  const relativePath = 'c6p_rapid_replace.rs';
  final fixture = File(p.join(workspace.path, relativePath));
  await fixture.writeAsString(
    _c6pDocument(language.id, lineCount),
    flush: true,
  );

  final issueSamples = <int>[];
  final finalQuerySamples = <int>[];
  final rssDeltas = <int>[];

  for (var sample = 0; sample < _sampleCount; sample++) {
    final opened = await _openNativeController(
      workspacePath: workspace.path,
      relativePath: relativePath,
      expectedMinimumLines: lineCount,
    );
    final controller = opened.controller;
    final rssBefore = ProcessInfo.currentRss;
    final issueWatch = Stopwatch()..start();
    for (var generation = 0; generation < replacementCount; generation++) {
      controller.configureNativeSyntaxDocument(
        languageId: language.id,
        documentId: '$relativePath#$sample#$generation',
      );
      await Future<void>.delayed(const Duration(milliseconds: 1));
    }
    issueWatch.stop();
    issueSamples.add(issueWatch.elapsedMicroseconds);

    final queryWatch = Stopwatch()..start();
    final spans = await controller.queryNativeSyntaxSpans(
      startLine: 0,
      endLine: _viewportLines - 1,
      overscan: 0,
    );
    queryWatch.stop();
    expect(spans, isNotNull);
    finalQuerySamples.add(queryWatch.elapsedMicroseconds);
    rssDeltas.add(ProcessInfo.currentRss - rssBefore);

    await tester.pumpWidget(const SizedBox.shrink());
    controller.dispose();
  }

  return <String, Object>{
    'language': language.id,
    'lines': lineCount,
    'replacement_count_per_sample': replacementCount,
    'replacement_issue_wall': _statsPayload(issueSamples),
    'final_generation_syntax_ready': _statsPayload(finalQuerySamples),
    'rss_delta_bytes_median': _medianInt(rssDeltas),
    'rss_delta_bytes_samples': rssDeltas,
    'note': 'Each configure starts an async retained parse; superseded generations are discarded only after openFromRope completes.',
  };
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
    if (!_aleraRustInitialized) {
      await alera_rust.RustLib.init();
      _aleraRustInitialized = true;
    }
    aleraInit.stop();
    final codeForgeInit = Stopwatch()..start();
    if (!_codeForgeRustInitialized) {
      await code_forge.RustLib.init();
      _codeForgeRustInitialized = true;
    }
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

  testWidgets('profiles C6P retained parser readiness matrix', (tester) async {
    await _ensureRustLibrariesInitialized();
    final workspace = await Directory.systemTemp.createTemp(
      'alera_c6p_parse_profile_',
    );
    addTearDown(() => workspace.delete(recursive: true));

    final cases = <Map<String, Object>>[];
    for (final language in _c6pLanguages) {
      for (final lineCount in _c6pLineCounts) {
        final result = await _measureC6pCase(
          tester,
          workspace: workspace,
          language: language,
          lineCount: lineCount,
        );
        cases.add(result);
        // ignore: avoid_print
        print('C6P_FLUTTER_PROFILE ${jsonEncode(result)}');
      }
    }

    final rapidReplacement = await _measureRapidReplacement(
      tester,
      workspace: workspace,
    );
    // ignore: avoid_print
    print('C6P_RAPID_REPLACEMENT ${jsonEncode(rapidReplacement)}');

    expect(cases, hasLength(_c6pLanguages.length * _c6pLineCounts.length));
    for (final result in cases) {
      expect(result['source_info_payload_proxy_bytes_max'], lessThan(1024));
    }
  }, timeout: const Timeout(Duration(minutes: 30)));
}
