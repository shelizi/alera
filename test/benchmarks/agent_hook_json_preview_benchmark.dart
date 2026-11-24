/// Run with the Dart VM from a prepared checkout. This compares the previous
/// full JSON encoding path against the bounded JSON preview path.
import 'dart:convert';

import 'package:alera/src/features/agent_status/infra/agent_hook_text_preview.dart';

typedef Preview = String? Function(Object?, int);

String? legacyPreview(Object? value, int limit) {
  try {
    return normalizeAgentHookSingleLine(jsonEncode(value), limit);
  } catch (_) {
    return null;
  }
}

void compare(String label, Object? value, int calls) {
  const limit = 160;
  final encoded = jsonEncode(value);
  final expected = legacyPreview(value, limit)!;
  if (normalizeAgentHookJsonPreview(value, limit) != expected) {
    throw StateError('Preview mismatch');
  }

  double measure(Preview preview) {
    var checksum = 0;
    final watch = Stopwatch()..start();
    for (var i = 0; i < calls; i++) {
      checksum += preview(value, limit)!.length;
    }
    watch.stop();
    if (checksum != expected.length * calls) {
      throw StateError('Checksum mismatch');
    }
    return watch.elapsedTicks * 1000 / watch.frequency;
  }

  for (var i = 0; i < 3; i++) {
    measure(legacyPreview);
    measure(normalizeAgentHookJsonPreview);
  }
  final before = <double>[];
  final after = <double>[];
  for (var i = 0; i < 7; i++) {
    if (i.isEven) {
      before.add(measure(legacyPreview));
      after.add(measure(normalizeAgentHookJsonPreview));
    } else {
      after.add(measure(normalizeAgentHookJsonPreview));
      before.add(measure(legacyPreview));
    }
  }
  before.sort();
  after.sort();
  print(
    '$label calls=$calls encoded_units=' +
        encoded.length.toString() +
        ' preview_units=' +
        expected.length.toString() +
        ' old_ms=' +
        before[3].toStringAsFixed(4) +
        ' new_ms=' +
        after[3].toStringAsFixed(4),
  );
}

void main() {
  compare('large_plain_string', {'result': '界😀abc' * 150000}, 24);
  compare('large_escaped_string', {
    'result': 'diagnostic "test"\\\r\n界😀\u2028\u2029' * 25000,
  }, 24);
  compare('large_nested_array', {
    'result': List.generate(
      16000,
      (i) => {'n': i, 'ok': true, 'name': 'item$i'},
    ),
  }, 24);
  compare('small_map', {
    'name': 'example',
    'args': [1, '界😀', true],
  }, 25000);
}
