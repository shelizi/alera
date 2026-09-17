/// Direct-replica versus xterm-facade benchmark for terminal search.
///
/// This measures the T4 source cutover against the legacy facade on identical
/// worker-produced buffer deltas. It deliberately has no performance threshold:
/// the numbers are hardware-dependent and are meant for before/after comparison.
///
/// Deliberately kept under `benchmark/`, so normal test sweeps do not run this
/// hardware-dependent benchmark.
///
///     flutter test benchmark/terminal_search_benchmark.dart
library;

import 'package:alera/src/features/workbench/presentation/terminal_search_controller.dart';
import 'package:alera/src/features/workbench/presentation/terminal_search_source.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

const _columns = 120;
const _rows = 40;
const _query = 'needle';
const _scanWarmupQuery = '__t4_search_warmup_no_match__';
const _navigationWarmupSteps = 100;
const _navigationSteps = 1000;

void main() {
  test('terminal search direct replica source cost', () async {
    final scenarios = <_Scenario>[
      const _Scenario('10k low density', lineCount: 10000, matchEvery: 1000),
      const _Scenario('10k high density', lineCount: 10000, matchEvery: 4),
      const _Scenario('100k low density', lineCount: 100000, matchEvery: 1000),
      const _Scenario('100k high density', lineCount: 100000, matchEvery: 4),
    ];

    // ignore: avoid_print
    print('\n=== terminal search source benchmark ===');
    for (final scenario in scenarios) {
      final payload = _buildPayload(scenario);
      final worker = await TerminalXtermWorker.start(
        cols: _columns,
        rows: _rows,
        maxLines: scenario.lineCount + _rows + 8,
      );
      final directReplica = TerminalXtermReplicaTerminal(
        cols: _columns,
        rows: _rows,
        maxLines: scenario.lineCount + _rows + 8,
      );
      final legacyReplica = TerminalXtermReplicaTerminal(
        cols: _columns,
        rows: _rows,
        maxLines: scenario.lineCount + _rows + 8,
      );

      TerminalSearchController? directController;
      TerminalSearchController? legacyController;
      try {
        final initial = await worker.writeBufferDelta(payload);
        directReplica.applyBufferDelta(initial);
        legacyReplica.applyBufferDelta(initial);

        final direct = _measureSearch(directReplica.replicaModel);
        final legacy = _measureSearch(XtermTerminalSearchSource(legacyReplica));
        directController = direct.controller;
        legacyController = legacy.controller;

        expect(direct.matchCount, legacy.matchCount);
        expect(direct.matchCount, greaterThan(0));

        final activeWarmup = await worker.writeBufferDelta(
          '\r\nactive warmup output for ${scenario.name}',
        );
        directReplica.applyBufferDelta(activeWarmup);
        legacyReplica.applyBufferDelta(activeWarmup);
        expect(direct.controller.matchCount, direct.matchCount);
        expect(legacy.controller.matchCount, legacy.matchCount);

        final update = await worker.writeBufferDelta(
          '\r\nactive $_query output for ${scenario.name}',
        );
        final directActive = _measureApply(
          () => directReplica.applyBufferDelta(update),
        );
        final legacyActive = _measureApply(
          () => legacyReplica.applyBufferDelta(update),
        );

        expect(direct.controller.matchCount, direct.matchCount + 1);
        expect(legacy.controller.matchCount, legacy.matchCount + 1);
        expect(direct.controller.matchCount, legacy.controller.matchCount);

        final report = _ScenarioReport(
          scenario: scenario,
          matches: direct.matchCount,
          directScan: direct.scan,
          legacyScan: legacy.scan,
          directNavigation: direct.navigation,
          legacyNavigation: legacy.navigation,
          directActiveOutput: directActive,
          legacyActiveOutput: legacyActive,
        );
        // ignore: avoid_print
        print(report);
      } finally {
        directController?.dispose();
        legacyController?.dispose();
        directReplica.dispose();
        legacyReplica.dispose();
        await worker.close();
      }
    }
  }, timeout: const Timeout(Duration(minutes: 2)));
}

_Measurement _measureSearch(TerminalSearchSource source) {
  final controller = TerminalSearchController.fromSource(
    source: source,
    scrollToLine: (_) {},
  );
  controller
    ..open()
    ..setQuery(_scanWarmupQuery)
    ..setQuery('');

  final scanWatch = Stopwatch()..start();
  controller.setQuery(_query);
  scanWatch.stop();
  final matchCount = controller.matchCount;

  for (var index = 0; index < _navigationWarmupSteps; index++) {
    controller
      ..next()
      ..previous();
  }

  final navigationWatch = Stopwatch()..start();
  for (var index = 0; index < _navigationSteps; index++) {
    controller
      ..next()
      ..previous();
  }
  navigationWatch.stop();

  return _Measurement(
    controller: controller,
    scan: scanWatch.elapsed,
    navigation: navigationWatch.elapsed,
    matchCount: matchCount,
  );
}

Duration _measureApply(void Function() apply) {
  final stopwatch = Stopwatch()..start();
  apply();
  stopwatch.stop();
  return stopwatch.elapsed;
}

String _buildPayload(_Scenario scenario) {
  final buffer = StringBuffer();
  for (var index = 0; index < scenario.lineCount; index++) {
    final hit = index % scenario.matchEvery == 0;
    buffer
      ..write('line ')
      ..write(index.toString().padLeft(6, '0'))
      ..write(' worker=')
      ..write(index % 17)
      ..write(' status=running ')
      ..write(hit ? 'needle matched' : 'ordinary output');
    if (index + 1 < scenario.lineCount) {
      buffer.write('\r\n');
    }
  }
  return buffer.toString();
}

final class _Scenario {
  const _Scenario(
    this.name, {
    required this.lineCount,
    required this.matchEvery,
  });

  final String name;
  final int lineCount;
  final int matchEvery;
}

final class _Measurement {
  const _Measurement({
    required this.controller,
    required this.scan,
    required this.navigation,
    required this.matchCount,
  });

  final TerminalSearchController controller;
  final Duration scan;
  final Duration navigation;
  final int matchCount;
}

final class _ScenarioReport {
  const _ScenarioReport({
    required this.scenario,
    required this.matches,
    required this.directScan,
    required this.legacyScan,
    required this.directNavigation,
    required this.legacyNavigation,
    required this.directActiveOutput,
    required this.legacyActiveOutput,
  });

  final _Scenario scenario;
  final int matches;
  final Duration directScan;
  final Duration legacyScan;
  final Duration directNavigation;
  final Duration legacyNavigation;
  final Duration directActiveOutput;
  final Duration legacyActiveOutput;

  String _millis(Duration value) =>
      (value.inMicroseconds / 1000).toStringAsFixed(2);

  String _ratio(Duration direct, Duration legacy) {
    if (direct.inMicroseconds == 0) return 'n/a';
    return (legacy.inMicroseconds / direct.inMicroseconds).toStringAsFixed(2);
  }

  @override
  String toString() {
    return '${scenario.name}: lines=${scenario.lineCount} matches=$matches '
        'scan direct=${_millis(directScan)}ms legacy=${_millis(legacyScan)}ms '
        'legacy/direct=${_ratio(directScan, legacyScan)}x; '
        'nav(${_navigationSteps * 2} moves) '
        'direct=${_millis(directNavigation)}ms '
        'legacy=${_millis(legacyNavigation)}ms '
        'legacy/direct=${_ratio(directNavigation, legacyNavigation)}x; '
        'active direct=${directActiveOutput.inMicroseconds}us '
        'legacy=${legacyActiveOutput.inMicroseconds}us '
        'legacy/direct=${_ratio(directActiveOutput, legacyActiveOutput)}x';
  }
}
