/// Benchmarks the Dart-owned terminal Scrollback search projection on a
/// deterministic mutable search source, without worker transport or raster.
/// Run: flutter test --no-pub benchmark/terminal_search_incremental_benchmark.dart
///
/// There are no hard timing thresholds. Compare the same Windows machine,
/// Flutter build mode and fixture before and after a code change.
library;

import 'package:alera/src/features/workbench/presentation/terminal_search_controller.dart';
import 'package:alera/src/features/workbench/presentation/terminal_search_source.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('100k scrollback search cost, high/low match density', () {
    for (final hitEvery in <int>[4, 1000]) {
      final scenario = hitEvery == 4 ? 'dense' : 'sparse';
      final initialTimes = <int>[];
      final appendTimes = <int>[];
      final rewriteTimes = <int>[];
      var matchCount = 0;
      for (var run = 0; run < 7; run++) {
        final source = _FakeSearchSource(
          List<_FakeLine>.generate(
            100000,
            (i) => _FakeLine(
              'line $i: ${i % hitEvery == 0 ? 'needle in log' : 'ordinary output'}',
              i,
            ),
          ),
        );
        final controller = TerminalSearchController.fromSource(
          source: source,
          scrollToLine: (_) {},
        );
        controller.open();
        try {
          var watch = Stopwatch()..start();
          controller.setQuery('needle');
          watch.stop();
          if (run > 0) initialTimes.add(watch.elapsedMicroseconds);
          matchCount = controller.matchCount;

          // Warm up the incremental branch before collecting the batch.
          source.addTail('extra warmup');
          var elapsed = 0;
          for (var i = 0; i < 20; i++) {
            watch = Stopwatch()..start();
            source.addTail(i.isEven ? 'needle extra $i' : 'ordinary extra $i');
            watch.stop();
            elapsed += watch.elapsedMicroseconds;
          }
          if (run > 0) appendTimes.add(elapsed ~/ 20);

          elapsed = 0;
          for (var i = 0; i < 20; i++) {
            watch = Stopwatch()..start();
            source.rewriteTail(
              i.isEven ? 'needle tui update' : 'ordinary tui update',
            );
            watch.stop();
            elapsed += watch.elapsedMicroseconds;
          }
          if (run > 0) rewriteTimes.add(elapsed ~/ 20);
        } finally {
          controller.dispose();
          source.dispose();
        }
      }
      initialTimes.sort();
      appendTimes.sort();
      rewriteTimes.sort();
      // ignore: avoid_print
      print(
        'TERMINAL_SEARCH_BENCH scenario=$scenario lines=100000 '
        'matches=$matchCount scan_med_us=${initialTimes[initialTimes.length ~/ 2]} '
        'append_med_us=${appendTimes[appendTimes.length ~/ 2]} '
        'rewrite_med_us=${rewriteTimes[rewriteTimes.length ~/ 2]}',
      );
    }
  }, timeout: const Timeout(Duration(minutes: 2)));
}

final class _FakeLine {
  _FakeLine(this.text, this.index);

  String text;
  int index;
}

final class _FakeSearchSource extends ChangeNotifier
    implements TerminalSearchSource {
  _FakeSearchSource(this.lines);

  final List<_FakeLine> lines;
  final Object _identity = Object();

  @override
  int get height => lines.length;

  @override
  int get viewHeight => 40;

  @override
  int get viewWidth => 120;

  @override
  Object get bufferIdentity => _identity;

  @override
  Object lineIdAt(int index) => lines[index];

  @override
  String lineTextAt(int index) => lines[index].text;

  @override
  int? lineIndexOf(Object lineId) {
    if (lineId is! _FakeLine) return null;
    final index = lineId.index;
    if (index < 0 || index >= lines.length) return null;
    return identical(lines[index], lineId) ? index : null;
  }

  void addTail(String text) {
    lines.add(_FakeLine(text, lines.length));
    notifyListeners();
  }

  void rewriteTail(String text) {
    lines.last.text = text;
    notifyListeners();
  }
}
