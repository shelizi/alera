/// Run with the Dart VM from a prepared checkout. Measures preview formatting,
/// not JSON decoding, Hook transport or widget rendering.
import 'package:alera/src/features/agent_status/infra/agent_hook_text_preview.dart';

typedef Preview = String Function(String, int);

String legacySingle(String text, int limit) {
  final normalized = text.trim().replaceAll(
    RegExp(r'[\r\n\u2028\u2029]+'),
    ' ',
  );
  return normalized.length <= limit
      ? normalized
      : normalized.substring(0, limit);
}

String legacyMultiline(String text, int limit) {
  final normalized = text
      .trim()
      .replaceAll('\r\n', '\n')
      .replaceAll('\r', '\n')
      .replaceAll(RegExp(r'[\u2028\u2029]'), '\n')
      .replaceAll(RegExp(r'\n{3,}'), '\n\n');
  return normalized.length <= limit
      ? normalized
      : normalized.substring(0, limit);
}

void compare(
  String label,
  List<String> inputs,
  int limit,
  int repetitions,
  Preview old,
  Preview current,
) {
  final expected = inputs.map((text) => old(text, limit)).toList();
  for (var index = 0; index < inputs.length; index++) {
    if (current(inputs[index], limit) != expected[index]) {
      throw StateError('Preview mismatch: $label/$index');
    }
  }
  final expectedSum =
      expected.fold<int>(0, (sum, text) => sum + text.length) * repetitions;

  double measure(Preview preview) {
    var sum = 0;
    final watch = Stopwatch()..start();
    for (var run = 0; run < repetitions; run++) {
      for (final text in inputs) {
        sum += preview(text, limit).length;
      }
    }
    watch.stop();
    if (sum != expectedSum) throw StateError('Unexpected checksum');
    return watch.elapsedTicks * 1000 / watch.frequency;
  }

  for (var i = 0; i < 3; i++) {
    measure(old);
    measure(current);
  }
  final before = <double>[];
  final after = <double>[];
  for (var i = 0; i < 7; i++) {
    if (i.isEven) {
      before.add(measure(old));
      after.add(measure(current));
    } else {
      after.add(measure(current));
      before.add(measure(old));
    }
  }
  before.sort();
  after.sort();
  print(
    '$label units=' +
        inputs.first.length.toString() +
        ' calls=' +
        (inputs.length * repetitions).toString() +
        ' limit=$limit old_ms=' +
        before[3].toStringAsFixed(4) +
        ' new_ms=' +
        after[3].toStringAsFixed(4),
  );
}

void main() {
  final log =
      'build started\r\n' +
      'compiler: diagnostic 界😀\r\n\r\n\r\n' * 30000 +
      'build finished\r\n';
  compare(
    'large_single',
    [log],
    160,
    12,
    legacySingle,
    normalizeAgentHookSingleLine,
  );
  compare(
    'large_multiline',
    [log],
    8000,
    12,
    legacyMultiline,
    normalizeAgentHookMultiline,
  );
  final short = [
    'Run all unit tests',
    'git status --short',
    'PermissionRequest',
    '  flutter test\r\n  dart analyze  ',
    'Fix Unicode 界😀',
  ];
  compare(
    'short_single',
    short,
    200,
    10000,
    legacySingle,
    normalizeAgentHookSingleLine,
  );
}
