/// Run with dart run test/benchmarks/agent_context_snapshot_benchmark.dart.
/// Measures bounded Context normalization and immutable snapshot creation,
/// excluding socket transport, JSON decoding and widget rendering.
import 'dart:convert';

import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';

// Previous snapshot construction, before raw source signature reuse.
Map<String, dynamic> legacySnapshot(Object? raw) {
  final overview = AgentContextOverview.fromRaw(raw);
  final json = overview.toJson();
  json['timeline'] = List<Map<String, Object?>>.unmodifiable(
    (json['timeline'] as List<Map<String, Object?>>).map(
      (event) => Map<String, Object?>.unmodifiable(event),
    ),
  );
  return Map<String, dynamic>.unmodifiable(json);
}

void main() {
  final encoded = jsonEncode({
    'goal': 'Fix Windows IME handling and terminal input 界😀 ' * 10,
    'progress': 'Implementation and regression tests completed. ' * 8,
    'initialGoal': 'Fix Windows IME',
    'currentFocus': 'Check input latency',
    'focusUpdatedAt': '2026-11-24T12:00:00+00:00',
    'timeline': [
      for (var i = 0; i < 16; i++)
        {
          'kind': 'report',
          'text':
              'Step $i: checked multilingual input and regression tests 界😀 ' *
              4,
          'at': '2026-11-24T12:00:${i.toString().padLeft(2, '0')}+00:00',
        },
    ],
  });
  // Distinct decoded maps model repeated full Host payloads. Parsing is
  // deliberately outside the timer to isolate the normalization cost.
  final inputs = [for (var i = 0; i < 32; i++) jsonDecode(encoded)];
  final expected = legacySnapshot(inputs.first);
  final check = AgentContextOverview.snapshotFromRaw(inputs.first);
  if (jsonEncode(check) != jsonEncode(expected)) {
    throw StateError('Snapshot mismatch');
  }
  const repetitions = 400;
  final expectedSum =
      (expected['goal'] as String).length * inputs.length * repetitions;
  double measure(bool cached) {
    var checksum = 0;
    Map<String, dynamic>? previous;
    final watch = Stopwatch()..start();
    for (var run = 0; run < repetitions; run++) {
      for (final input in inputs) {
        final next = cached
            ? AgentContextOverview.snapshotFromRaw(input, previous: previous)
            : legacySnapshot(input);
        checksum += (next['goal'] as String).length;
        previous = next;
      }
    }
    watch.stop();
    if (checksum != expectedSum) throw StateError('Checksum mismatch');
    return watch.elapsedTicks * 1000 / watch.frequency;
  }

  for (var i = 0; i < 2; i++) {
    measure(false);
    measure(true);
  }
  final before = <double>[];
  final after = <double>[];
  for (var i = 0; i < 5; i++) {
    if (i.isEven) {
      before.add(measure(false));
      after.add(measure(true));
    } else {
      after.add(measure(true));
      before.add(measure(false));
    }
  }
  before.sort();
  after.sort();
  print(
    'context_snapshot calls=${inputs.length * repetitions} '
    'old_ms=${before[2].toStringAsFixed(3)} '
    'new_ms=${after[2].toStringAsFixed(3)}',
  );
}
