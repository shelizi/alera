/// Measures the current CodeForge regex-search pipeline on real desktop FFI.
///
/// The benchmark intentionally separates:
/// - native Rope -> Dart String snapshot/FFI cost;
/// - Dart RegExp scan + scalar-offset conversion on the current isolate;
/// - `compute()` isolate transfer + the same RegExp work;
/// - the production-shaped snapshot + `compute()` end-to-end path;
/// - regex replace-all, where a full replacement String also crosses back.
///
/// This is a hardware-dependent comparison benchmark, not a CI timing gate.
/// Run at least five samples on the same machine/build mode before using the
/// numbers for an architecture decision.
///
///     flutter test integration_test/editor_regex_search_benchmark.dart -d windows
///     flutter test integration_test/editor_regex_search_benchmark.dart -d linux
library;

import 'dart:convert';
import 'dart:math';

import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:code_forge/code_forge/rope.dart';
import 'package:code_forge/code_forge/versioned_text_snapshot_cache.dart';
import 'package:flutter/foundation.dart' show compute;
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

const _measuredRuns = 5;
const _smallBytes = 512 * 1024;
const _largeBytes = 4 * 1024 * 1024;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('editor regex snapshot/copy benchmark', (tester) async {
    await code_forge.RustLib.init();

    _verifyCompatibilityFixtures();
    await compute(code_forge.computeRegexSearchRanges, (
      text: 'warmup foo123',
      query: r'foo\d+',
      caseSensitive: true,
      matchWholeWord: false,
    ));

    final scenarios = <_Scenario>[
      _Scenario(
        name: '512KiB ascii low density',
        text: _buildLowDensityAscii(_smallBytes),
        query: r'needle_[0-9]+',
        replacement: 'target_0001',
        measureReplaceAll: false,
      ),
      _Scenario(
        name: '512KiB ascii high density',
        text: _buildHighDensityAscii(_smallBytes),
        query: r'foo\d+',
        replacement: 'bar456',
        measureReplaceAll: false,
      ),
      _Scenario(
        name: '4MiB ascii low density',
        text: _buildLowDensityAscii(_largeBytes),
        query: r'needle_[0-9]+',
        replacement: 'target_0001',
        measureReplaceAll: true,
      ),
      _Scenario(
        name: '4MiB ascii high density',
        text: _buildHighDensityAscii(_largeBytes),
        query: r'foo\d+',
        replacement: 'bar456',
        measureReplaceAll: true,
      ),
      _Scenario(
        name: '4MiB unicode/scalar offsets',
        text: _buildUnicodeDensity(_largeBytes),
        query: r'foo\d+',
        replacement: 'bar456',
        measureReplaceAll: true,
      ),
      _Scenario(
        name: '512KiB lookahead compatibility',
        text: _buildLookaheadDensity(_smallBytes),
        query: r'foo(?=bar)',
        replacement: 'FOO',
        measureReplaceAll: false,
      ),
    ];

    // ignore: avoid_print
    print('\n=== editor regex snapshot/copy benchmark ===');
    for (final scenario in scenarios) {
      final report = await _runScenario(scenario);
      // ignore: avoid_print
      print(report);
      expect(report.matchCount, greaterThan(0));
      expect(report.snapshot.samples, hasLength(_measuredRuns));
      expect(report.directRegex.samples, hasLength(_measuredRuns));
      expect(report.isolateRegex.samples, hasLength(_measuredRuns));
      expect(report.endToEnd.samples, hasLength(_measuredRuns));
      await tester.pump();

      if (scenario.measureReplaceAll) {
        final replaceReport = await _runReplaceAllScenario(scenario);
        // ignore: avoid_print
        print(replaceReport);
        expect(replaceReport.directReplace.samples, hasLength(_measuredRuns));
        expect(replaceReport.isolateReplace.samples, hasLength(_measuredRuns));
        expect(replaceReport.endToEnd.samples, hasLength(_measuredRuns));
        await tester.pump();
      }
    }

    final repeated = await _runRepeatedQueryBenchmark(
      _buildHighDensityAscii(_largeBytes),
    );
    // ignore: avoid_print
    print(repeated);
    expect(repeated.freshSnapshotSequence.samples, hasLength(_measuredRuns));
    expect(repeated.cachedSnapshotSequence.samples, hasLength(_measuredRuns));
  });
}

Future<_ScenarioReport> _runScenario(_Scenario scenario) async {
  final rope = Rope(scenario.text);
  final request = (
    text: scenario.text,
    query: scenario.query,
    caseSensitive: true,
    matchWholeWord: false,
  );

  final baselineRanges = code_forge.computeRegexSearchRanges(request);
  final expectedCount = baselineRanges.length;
  expect(expectedCount, greaterThan(0));

  // Untimed warm-ups keep one-off JIT/loader effects out of the five samples.
  final warmSnapshot = await rope.getTextSnapshot();
  expect(warmSnapshot.length, scenario.text.length);
  code_forge.computeRegexSearchRanges(request);
  final warmIsolate = await compute(
    code_forge.computeRegexSearchRanges,
    request,
  );
  expect(warmIsolate.length, expectedCount);

  final snapshot = <int>[];
  final directRegex = <int>[];
  final isolateRegex = <int>[];
  final endToEnd = <int>[];

  for (var run = 0; run < _measuredRuns; run++) {
    var stopwatch = Stopwatch()..start();
    final snapshotText = await rope.getTextSnapshot();
    stopwatch.stop();
    expect(snapshotText.length, scenario.text.length);
    snapshot.add(stopwatch.elapsedMicroseconds);

    stopwatch = Stopwatch()..start();
    final direct = code_forge.computeRegexSearchRanges((
      text: snapshotText,
      query: scenario.query,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(direct.length, expectedCount);
    directRegex.add(stopwatch.elapsedMicroseconds);

    stopwatch = Stopwatch()..start();
    final isolated = await compute(code_forge.computeRegexSearchRanges, (
      text: snapshotText,
      query: scenario.query,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(isolated.length, expectedCount);
    isolateRegex.add(stopwatch.elapsedMicroseconds);

    stopwatch = Stopwatch()..start();
    final endToEndText = await rope.getTextSnapshot();
    final endToEndRanges = await compute(code_forge.computeRegexSearchRanges, (
      text: endToEndText,
      query: scenario.query,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(endToEndRanges.length, expectedCount);
    endToEnd.add(stopwatch.elapsedMicroseconds);
  }

  return _ScenarioReport(
    scenario: scenario,
    utf8Bytes: utf8.encode(scenario.text).length,
    codeUnits: scenario.text.length,
    scalarCount: scenario.text.runes.length,
    matchCount: expectedCount,
    snapshot: _Stats(snapshot),
    directRegex: _Stats(directRegex),
    isolateRegex: _Stats(isolateRegex),
    endToEnd: _Stats(endToEnd),
  );
}

Future<_ReplaceReport> _runReplaceAllScenario(_Scenario scenario) async {
  final rope = Rope(scenario.text);
  final request = (
    text: scenario.text,
    query: scenario.query,
    replacement: scenario.replacement,
    isRegex: true,
    caseSensitive: true,
    matchWholeWord: false,
  );
  final expected = code_forge.computeReplaceAllText(request);
  expect(expected, isNot(equals(scenario.text)));

  code_forge.computeReplaceAllText(request);
  final warm = await compute(code_forge.computeReplaceAllText, request);
  expect(warm, expected);
  await rope.getTextSnapshot();

  final direct = <int>[];
  final isolated = <int>[];
  final endToEnd = <int>[];

  for (var run = 0; run < _measuredRuns; run++) {
    final snapshotText = await rope.getTextSnapshot();

    var stopwatch = Stopwatch()..start();
    final directText = code_forge.computeReplaceAllText((
      text: snapshotText,
      query: scenario.query,
      replacement: scenario.replacement,
      isRegex: true,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(directText, expected);
    direct.add(stopwatch.elapsedMicroseconds);

    stopwatch = Stopwatch()..start();
    final isolateText = await compute(code_forge.computeReplaceAllText, (
      text: snapshotText,
      query: scenario.query,
      replacement: scenario.replacement,
      isRegex: true,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(isolateText, expected);
    isolated.add(stopwatch.elapsedMicroseconds);

    stopwatch = Stopwatch()..start();
    final endToEndSnapshot = await rope.getTextSnapshot();
    final replaced = await compute(code_forge.computeReplaceAllText, (
      text: endToEndSnapshot,
      query: scenario.query,
      replacement: scenario.replacement,
      isRegex: true,
      caseSensitive: true,
      matchWholeWord: false,
    ));
    stopwatch.stop();
    expect(replaced, expected);
    endToEnd.add(stopwatch.elapsedMicroseconds);
  }

  return _ReplaceReport(
    scenario: scenario,
    outputUtf8Bytes: utf8.encode(expected).length,
    directReplace: _Stats(direct),
    isolateReplace: _Stats(isolated),
    endToEnd: _Stats(endToEnd),
  );
}

Future<_RepeatedQueryReport> _runRepeatedQueryBenchmark(String text) async {
  final rope = Rope(text);
  const queries = <String>[r'f', r'fo', r'foo', r'foo\d', r'foo\d+'];

  // Warm the isolate helper once with the same multi-MiB payload.
  final warmText = await rope.getTextSnapshot();
  await compute(code_forge.computeRegexSearchRanges, (
    text: warmText,
    query: queries.last,
    caseSensitive: true,
    matchWholeWord: false,
  ));

  final freshSnapshotSequence = <int>[];
  final cachedSnapshotSequence = <int>[];
  for (var run = 0; run < _measuredRuns; run++) {
    var stopwatch = Stopwatch()..start();
    for (final query in queries) {
      final snapshot = await rope.getTextSnapshot();
      await compute(code_forge.computeRegexSearchRanges, (
        text: snapshot,
        query: query,
        caseSensitive: true,
        matchWholeWord: false,
      ));
    }
    stopwatch.stop();
    freshSnapshotSequence.add(stopwatch.elapsedMicroseconds);

    final cache = VersionedTextSnapshotCache();
    stopwatch = Stopwatch()..start();
    for (final query in queries) {
      final snapshot = await cache.get(version: 1, load: rope.getTextSnapshot);
      await compute(code_forge.computeRegexSearchRanges, (
        text: snapshot,
        query: query,
        caseSensitive: true,
        matchWholeWord: false,
      ));
    }
    stopwatch.stop();
    cachedSnapshotSequence.add(stopwatch.elapsedMicroseconds);
  }

  return _RepeatedQueryReport(
    utf8Bytes: utf8.encode(text).length,
    queries: queries,
    freshSnapshotSequence: _Stats(freshSnapshotSequence),
    cachedSnapshotSequence: _Stats(cachedSnapshotSequence),
  );
}

void _verifyCompatibilityFixtures() {
  expect(
    code_forge.computeRegexSearchRanges((
      text: 'foobar fooqux',
      query: r'foo(?=bar)',
      caseSensitive: true,
      matchWholeWord: false,
    )),
    equals(<(int, int)>[(0, 3)]),
  );
  expect(
    code_forge.computeRegexSearchRanges((
      text: 'foofoo foo',
      query: r'(foo)\1',
      caseSensitive: true,
      matchWholeWord: false,
    )),
    equals(<(int, int)>[(0, 6)]),
  );
  expect(
    code_forge.computeRegexSearchRanges((
      text: 'a😀x😀z',
      query: '😀',
      caseSensitive: true,
      matchWholeWord: false,
    )),
    equals(<(int, int)>[(1, 2), (3, 4)]),
  );
  expect(
    code_forge.computeRegexSearchRanges((
      text: 'foo foo_bar xfoo foo',
      query: 'foo',
      caseSensitive: true,
      matchWholeWord: true,
    )),
    equals(<(int, int)>[(0, 3), (17, 20)]),
  );
  expect(
    code_forge.computeReplaceAllText((
      text: 'foobar fooqux foobar',
      query: r'foo(?=bar)',
      replacement: 'X',
      isRegex: true,
      caseSensitive: true,
      matchWholeWord: false,
    )),
    'Xbar fooqux Xbar',
  );
}

String _buildLowDensityAscii(int targetBytes) {
  const ordinary =
      'INFO worker=07 package=workspace status=running payload=abcdefghijklmnopqrstuvwxyz0123456789\n';
  const matching =
      'WARN worker=07 needle_0001 package=workspace status=running payload=abcdefghijklmnopqrstuvwxyz\n';
  return _repeatPatternToBytes(
    targetBytes,
    (index) => index % 128 == 0 ? matching : ordinary,
  );
}

String _buildHighDensityAscii(int targetBytes) {
  const line =
      'foo123 alpha beta gamma delta epsilon 0123456789 payload payload payload\n';
  return _repeatPatternToBytes(targetBytes, (_) => line);
}

String _buildUnicodeDensity(int targetBytes) {
  const line = '前綴😀 foo123 中間資料 café λ 日本語 한글 結尾 payload payload\n';
  return _repeatPatternToBytes(targetBytes, (_) => line);
}

String _buildLookaheadDensity(int targetBytes) {
  const line =
      'foobar fooqux foozap alpha beta gamma delta epsilon payload 0123456789\n';
  return _repeatPatternToBytes(targetBytes, (_) => line);
}

String _repeatPatternToBytes(
  int targetBytes,
  String Function(int index) lineForIndex,
) {
  final buffer = StringBuffer();
  var bytes = 0;
  var index = 0;
  while (bytes < targetBytes) {
    final line = lineForIndex(index++);
    buffer.write(line);
    bytes += utf8.encode(line).length;
  }
  return buffer.toString();
}

final class _Scenario {
  const _Scenario({
    required this.name,
    required this.text,
    required this.query,
    required this.replacement,
    required this.measureReplaceAll,
  });

  final String name;
  final String text;
  final String query;
  final String replacement;
  final bool measureReplaceAll;
}

final class _Stats {
  _Stats(List<int> samples) : samples = List.unmodifiable(samples);

  final List<int> samples;

  double get medianMicros => _percentile(0.50);
  double get p95Micros => _percentile(0.95);

  double get madMicros {
    final median = medianMicros;
    final deviations = samples.map((value) => (value - median).abs()).toList()
      ..sort();
    return _medianSorted(deviations.map((value) => value.toDouble()).toList());
  }

  double _percentile(double fraction) {
    final values = samples.map((value) => value.toDouble()).toList()..sort();
    final index = ((values.length - 1) * fraction).round();
    return values[index];
  }

  static double _medianSorted(List<double> values) {
    if (values.isEmpty) return 0;
    final middle = values.length ~/ 2;
    if (values.length.isOdd) return values[middle];
    return (values[middle - 1] + values[middle]) / 2;
  }

  String get compact =>
      '${(medianMicros / 1000).toStringAsFixed(2)}ms '
      'p95=${(p95Micros / 1000).toStringAsFixed(2)}ms '
      'mad=${(madMicros / 1000).toStringAsFixed(2)}ms '
      'raw=${samples.map((value) => (value / 1000).toStringAsFixed(2)).toList()}';
}

final class _ScenarioReport {
  const _ScenarioReport({
    required this.scenario,
    required this.utf8Bytes,
    required this.codeUnits,
    required this.scalarCount,
    required this.matchCount,
    required this.snapshot,
    required this.directRegex,
    required this.isolateRegex,
    required this.endToEnd,
  });

  final _Scenario scenario;
  final int utf8Bytes;
  final int codeUnits;
  final int scalarCount;
  final int matchCount;
  final _Stats snapshot;
  final _Stats directRegex;
  final _Stats isolateRegex;
  final _Stats endToEnd;

  @override
  String toString() {
    final isolateOverhead = max(
      0,
      isolateRegex.medianMicros - directRegex.medianMicros,
    );
    final snapshotShare = endToEnd.medianMicros == 0
        ? 0
        : snapshot.medianMicros / endToEnd.medianMicros * 100;
    return '${scenario.name}: utf8=$utf8Bytes codeUnits=$codeUnits '
        'scalars=$scalarCount matches=$matchCount\n'
        '  snapshot=${snapshot.compact}\n'
        '  direct_regex=${directRegex.compact}\n'
        '  isolate_regex=${isolateRegex.compact}\n'
        '  end_to_end=${endToEnd.compact}\n'
        '  isolate_minus_direct≈${(isolateOverhead / 1000).toStringAsFixed(2)}ms '
        'snapshot_share≈${snapshotShare.toStringAsFixed(1)}%';
  }
}

final class _ReplaceReport {
  const _ReplaceReport({
    required this.scenario,
    required this.outputUtf8Bytes,
    required this.directReplace,
    required this.isolateReplace,
    required this.endToEnd,
  });

  final _Scenario scenario;
  final int outputUtf8Bytes;
  final _Stats directReplace;
  final _Stats isolateReplace;
  final _Stats endToEnd;

  @override
  String toString() =>
      '${scenario.name} replace-all: outputUtf8=$outputUtf8Bytes\n'
      '  direct_replace=${directReplace.compact}\n'
      '  isolate_replace=${isolateReplace.compact}\n'
      '  snapshot+isolate_replace=${endToEnd.compact}';
}

final class _RepeatedQueryReport {
  const _RepeatedQueryReport({
    required this.utf8Bytes,
    required this.queries,
    required this.freshSnapshotSequence,
    required this.cachedSnapshotSequence,
  });

  final int utf8Bytes;
  final List<String> queries;
  final _Stats freshSnapshotSequence;
  final _Stats cachedSnapshotSequence;

  @override
  String toString() {
    final saved =
        freshSnapshotSequence.medianMicros -
        cachedSnapshotSequence.medianMicros;
    return '4MiB repeated query edits: utf8=$utf8Bytes queries=$queries\n'
        '  current_fresh_snapshot_each_query=${freshSnapshotSequence.compact}\n'
        '  version_cached_snapshot_sequence=${cachedSnapshotSequence.compact}\n'
        '  snapshot_reuse_delta≈${(saved / 1000).toStringAsFixed(2)}ms/sequence';
  }
}
