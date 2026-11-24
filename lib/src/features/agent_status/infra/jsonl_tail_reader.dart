import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

const _defaultBlockBytes = 16 * 1024;

/// Search a bounded JSONL tail in reverse order, stopping on the first match.
///
/// The caller supplies positional reads of a single open file. Decode complete
/// lines, rather than individual blocks, so a UTF-8 character split across
/// blocks is preserved. At most one line's fragments are retained between
/// blocks; the full tail is never eagerly decoded or split into strings.
/// A short read signals a changed/truncated file and fails closed.
String? readLastMatchingJsonlLine({
  required int length,
  required int maxScanBytes,
  required Uint8List Function(int offset, int count) readAt,
  required String? Function(String line) extract,
  int blockBytes = _defaultBlockBytes,
}) {
  if (length <= 0 || maxScanBytes <= 0) return null;
  if (blockBytes <= 0) {
    throw ArgumentError.value(blockBytes, 'blockBytes');
  }
  final lowerBound = math.max(0, length - maxScanBytes);
  var end = length;
  // Stored in reverse file order, only for a line crossing block boundaries.
  final suffixes = <Uint8List>[];

  String? matchLine(Uint8List prefix) {
    Uint8List lineBytes;
    if (suffixes.isEmpty) {
      lineBytes = prefix;
    } else {
      final line = BytesBuilder(copy: false)..add(prefix);
      for (final suffix in suffixes.reversed) {
        line.add(suffix);
      }
      lineBytes = line.takeBytes();
      suffixes.clear();
    }
    if (lineBytes.isEmpty) return null;
    final line = utf8.decode(lineBytes, allowMalformed: true).trim();
    return line.isEmpty ? null : extract(line);
  }

  while (end > lowerBound) {
    final start = math.max(lowerBound, end - blockBytes);
    final bytes = readAt(start, end - start);
    if (bytes.length != end - start) return null;
    var segmentEnd = bytes.length;
    for (var index = bytes.length - 1; index >= 0; index--) {
      if (bytes[index] != 0x0a) continue;
      final result = matchLine(
        Uint8List.sublistView(bytes, index + 1, segmentEnd),
      );
      if (result != null) return result;
      segmentEnd = index;
    }
    if (segmentEnd > 0) {
      suffixes.add(Uint8List.sublistView(bytes, 0, segmentEnd));
    }
    end = start;
  }

  // Preserve the old bounded-tail behavior: also inspect the first fragment
  // inside the scan window and a complete final record without a newline.
  return suffixes.isEmpty ? null : matchLine(Uint8List(0));
}
