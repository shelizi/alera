// ignore_for_file: invalid_use_of_visible_for_testing_member, prefer_interpolation_to_compose_strings

import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:flutter_test/flutter_test.dart';

/// The previous full-document reference, kept in a manual benchmark only.
String _legacySemanticSource({
  required String currentDisplayText,
  required String originalRawText,
  required String originalDisplayText,
}) {
  if (currentDisplayText == originalDisplayText) return originalRawText;
  final rawSegments = _splitSegments(originalRawText);
  final displaySegments = _splitSegments(originalDisplayText);
  final currentSegments = _splitSegments(currentDisplayText);
  if (rawSegments.length != displaySegments.length) {
    return currentDisplayText;
  }
  var prefix = 0;
  while (prefix < displaySegments.length &&
      prefix < currentSegments.length &&
      displaySegments[prefix] == currentSegments[prefix]) {
    prefix++;
  }
  var suffix = 0;
  while (suffix < displaySegments.length - prefix &&
      suffix < currentSegments.length - prefix) {
    final originalIndex = displaySegments.length - suffix - 1;
    final currentIndex = currentSegments.length - suffix - 1;
    if (displaySegments[originalIndex] != currentSegments[currentIndex]) break;
    suffix++;
  }
  final encoded = StringBuffer();
  final suffixStart = currentSegments.length - suffix;
  for (var i = 0; i < currentSegments.length; i++) {
    if (i < prefix) {
      encoded.write(rawSegments[i]);
    } else if (i >= suffixStart) {
      encoded.write(
        rawSegments[rawSegments.length - (currentSegments.length - i)],
      );
    } else {
      encoded.write(currentSegments[i]);
    }
  }
  return encoded.toString();
}

List<String> _splitSegments(String text) {
  final segments = <String>[];
  var start = 0;
  for (var index = 0; index < text.length; index++) {
    final unit = text.codeUnitAt(index);
    if (unit == 0x0D) {
      if (index + 1 < text.length && text.codeUnitAt(index + 1) == 0x0A) {
        index++;
      }
      segments.add(text.substring(start, index + 1));
      start = index + 1;
    } else if (unit == 0x0A) {
      segments.add(text.substring(start, index + 1));
      start = index + 1;
    }
  }
  if (start < text.length) segments.add(text.substring(start));
  return segments;
}

/// CPU-only source conversion benchmark. No LSP transport, FFI, or repaint.
/// flutter test --no-pub benchmark/editor_semantic_sync_benchmark.dart
void main() {
  test('LSP source conversion median CPU time', () {
    for (final mode in <String>['no_tabs', 'preserved_tabs']) {
      for (final rows in <int>[10000, 40000]) {
        final rawBuffer = StringBuffer();
        final displayBuffer = StringBuffer();
        for (var i = 0; i < rows; i++) {
          final suffix = 'const item_' + i.toString() + ' = 123;\n';
          rawBuffer.write(mode == 'no_tabs' ? '    ' : '\t');
          rawBuffer.write(suffix);
          displayBuffer.write('    ');
          displayBuffer.write(suffix);
        }
        final raw = rawBuffer.toString();
        // Force equal-by-value, not identical, String instances in no-tab
        // cases so the benchmark doesn't rely on pointer-equality alone.
        final display = String.fromCharCodes(
          displayBuffer.toString().codeUnits,
        );
        final current = display.replaceFirst('item_0', 'item_0_changed');
        final reference = _legacySemanticSource(
          currentDisplayText: current,
          originalRawText: raw,
          originalDisplayText: display,
        );
        expect(
          workspaceEditorEncodeSemanticSourceText(
            currentDisplayText: current,
            originalRawText: raw,
            originalDisplayText: display,
          ),
          reference,
        );
        for (var warm = 0; warm < 2; warm++) {
          _legacySemanticSource(
            currentDisplayText: current,
            originalRawText: raw,
            originalDisplayText: display,
          );
          workspaceEditorEncodeSemanticSourceText(
            currentDisplayText: current,
            originalRawText: raw,
            originalDisplayText: display,
          );
        }
        final baselineUs = <int>[];
        final optimizedUs = <int>[];
        for (var iteration = 0; iteration < 7; iteration++) {
          void measure(bool legacy) {
            final watch = Stopwatch()..start();
            final result = legacy
                ? _legacySemanticSource(
                    currentDisplayText: current,
                    originalRawText: raw,
                    originalDisplayText: display,
                  )
                : workspaceEditorEncodeSemanticSourceText(
                    currentDisplayText: current,
                    originalRawText: raw,
                    originalDisplayText: display,
                  );
            watch.stop();
            expect(result, reference);
            (legacy ? baselineUs : optimizedUs).add(watch.elapsedMicroseconds);
          }

          if (iteration.isEven) {
            measure(true);
            measure(false);
          } else {
            measure(false);
            measure(true);
          }
        }
        baselineUs.sort();
        optimizedUs.sort();
        // ignore: avoid_print
        print(
          'PROFILE_LSP_TEXT mode=' +
              mode +
              ' rows=' +
              rows.toString() +
              ' chars=' +
              raw.length.toString() +
              ' old_median_us=' +
              baselineUs[3].toString() +
              ' new_median_us=' +
              optimizedUs[3].toString() +
              ' ratio=' +
              (baselineUs[3] / optimizedUs[3]).toStringAsFixed(1),
        );
      }
    }
  }, timeout: const Timeout(Duration(minutes: 2)));
}
