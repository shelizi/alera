// ignore_for_file: avoid_print, implementation_imports

/// Exercise the real generated stream transport against a private fixture.
/// dart run tool/performance/workspace_search_stream_probe.dart <native library>
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/rust/api/workspace_search.dart' as api;
import 'package:alera/src/rust/frb_generated.dart' as rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

void require(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Object fingerprint(
  List<api.WorkspaceSearchFileResult> files,
  int count,
  bool truncated,
) => [
  count,
  truncated,
  for (final file in files)
    [
      file.relativePath,
      file.contentToken,
      for (final match in file.matches)
        [
          match.id,
          match.line,
          match.column,
          match.matchLength,
          match.lineContent,
          match.displayColumn,
          match.displayMatchLength,
          match.replacementPreview,
        ],
    ],
];

Future<void> main(List<String> args) async {
  if (args.length != 1)
    throw ArgumentError('Supply the freshly built native library path');
  await rust.RustLib.init(externalLibrary: ExternalLibrary.open(args.single));
  final root = Directory.systemTemp.createTempSync('alera-search-stream-');
  try {
    const needle = 'progressive_fixture_needle';
    final ordinary = List.filled(
      220,
      'let value = ordinary_source_content;\n',
    ).join();
    for (var i = 0; i < 1200; i++) {
      final dir = Directory(
        '${root.path}/module_${(i ~/ 60).toString().padLeft(2, '0')}',
      );
      dir.createSync(recursive: true);
      File('${dir.path}/file_${i.toString().padLeft(4, '0')}.rs')
          .writeAsStringSync(i % 23 == 0 ? '$needle\n$ordinary' : ordinary);
    }
    api.WorkspaceSearchOptions options(
      String query, {
      bool regex = false,
      int limit = 2000,
    }) => api.WorkspaceSearchOptions(
      workspacePath: root.path,
      query: query,
      caseSensitive: true,
      wholeWord: false,
      useRegex: regex,
      includeIgnored: false,
      maxResults: limit,
    );
    Future<({int first, int total, Object result, int batches})> runStream(
      api.WorkspaceSearchOptions opts,
      String id,
    ) async {
      final watch = Stopwatch()..start();
      var first = -1;
      var batches = 0;
      var terminal = false;
      final files = <api.WorkspaceSearchFileResult>[];
      final paths = <String>{};
      var count = 0;
      var truncated = false;
      await for (final batch
          in api
              .searchWorkspaceStream(options: opts, requestId: id)
              .timeout(const Duration(seconds: 30))) {
        require(!terminal, 'Unexpected batch after terminal');
        if (batch.error case final error?) throw error;
        batches++;
        require(batch.files.length <= 16, 'Unbounded batch');
        if (first < 0 && batch.files.isNotEmpty)
          first = watch.elapsedMicroseconds;
        for (final file in batch.files) {
          require(paths.add(file.relativePath), 'Repeated path');
          files.add(file);
          count += file.matches.length;
        }
        require(count == batch.totalMatches, 'Count mismatch');
        truncated = batch.truncated;
        terminal = batch.done;
      }
      require(terminal, 'Missing terminal');
      return (
        first: first,
        total: watch.elapsedMicroseconds,
        result: fingerprint(files, count, truncated),
        batches: batches,
      );
    }

    final opts = options(needle);
    final initial = await api.searchWorkspace(options: opts);
    require(initial.totalMatches == 53, 'Fixture count mismatch');
    final expected = jsonEncode(
      fingerprint(initial.files, initial.totalMatches, initial.truncated),
    );
    final firstTimes = <int>[];
    final totalTimes = <int>[];
    final oldTimes = <int>[];
    for (var round = 0; round < 7; round++) {
      Future<void> old() async {
        final watch = Stopwatch()..start();
        final result = await api.searchWorkspaceCancelable(
          options: opts,
          requestId: 'old-$round',
        );
        oldTimes.add(watch.elapsedMicroseconds);
        require(
          jsonEncode(
                fingerprint(
                  result.files,
                  result.totalMatches,
                  result.truncated,
                ),
              ) ==
              expected,
          'Complete API differs',
        );
      }

      Future<void> streamed() async {
        final result = await runStream(opts, 'stream-$round');
        firstTimes.add(result.first);
        totalTimes.add(result.total);
        require(
          result.batches > 2 &&
              result.first >= 0 &&
              result.first < result.total,
          'No progressive result',
        );
        require(
          jsonEncode(result.result) == expected,
          'Stream reconstruction differs',
        );
      }

      if (round.isEven) {
        await old();
        await streamed();
      } else {
        await streamed();
        await old();
      }
    }
    final capped = options(needle, limit: 7);
    final reference = await api.searchWorkspace(options: capped);
    final streamed = await runStream(capped, 'cap');
    require(
      jsonEncode(streamed.result) ==
          jsonEncode(
            fingerprint(
              reference.files,
              reference.totalMatches,
              reference.truncated,
            ),
          ),
      'Cap mismatch',
    );
    final empty = await runStream(options('never_present_8cbe'), 'empty');
    require(
      empty.first == -1 && empty.batches == 1,
      'Empty result did not terminate',
    );
    final errors = await api
        .searchWorkspaceStream(
          options: options('[', regex: true),
          requestId: 'invalid',
        )
        .timeout(const Duration(seconds: 30))
        .toList();
    require(
      errors.length == 1 &&
          errors.single.done &&
          errors.single.error?.kind ==
              api.WorkspaceSearchErrorKind.invalidPattern,
      'Invalid regex did not report typed terminal error',
    );
    await api.cancelWorkspaceSearch(requestId: 'cancel-before-start');
    final cancelled = await api
        .searchWorkspaceStream(options: opts, requestId: 'cancel-before-start')
        .timeout(const Duration(seconds: 30))
        .toList();
    require(
      cancelled.length == 1 &&
          cancelled.single.done &&
          cancelled.single.error?.kind ==
              api.WorkspaceSearchErrorKind.cancelled,
      'Pre-start cancellation not preserved',
    );
    int median(List<int> values) => (values..sort())[values.length ~/ 2];
    print(
      jsonEncode({
        'files': 1200,
        'matches': initial.totalMatches,
        'rounds': 7,
        'old_complete_us': median(oldTimes),
        'stream_first_us': median(firstTimes),
        'stream_complete_us': median(totalTimes),
        'typed_error_and_cancel': true,
      }),
    );
  } finally {
    root.deleteSync(recursive: true);
  }
}
