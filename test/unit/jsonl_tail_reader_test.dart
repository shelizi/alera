import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:alera/src/features/agent_status/infra/jsonl_tail_reader.dart';
import 'package:flutter_test/flutter_test.dart';

String? _assistant(String line) {
  try {
    final decoded = jsonDecode(line);
    return decoded is Map && decoded['role'] == 'assistant'
        ? decoded['content'] as String?
        : null;
  } catch (_) {
    return null;
  }
}

String? _baseline(Uint8List bytes, int maxScanBytes) {
  final start = bytes.length > maxScanBytes ? bytes.length - maxScanBytes : 0;
  final lines = utf8
      .decode(bytes.sublist(start), allowMalformed: true)
      .split('\n');
  for (final raw in lines.reversed) {
    final result = _assistant(raw.trim());
    if (result != null) return result;
  }
  return null;
}

void main() {
  test(
    'recent assistant record reads only one block of a real 4 MB tail',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'alera-jsonl-tail-',
      );
      addTearDown(() => directory.delete(recursive: true));
      final file = File('${directory.path}/history.jsonl');
      final prefix = jsonEncode({'role': 'tool', 'content': 'x' * 4096}) + '\n';
      await file.writeAsString(
        prefix * 1100 + '{"role":"assistant","content":"完成驗證"}\n',
      );
      final handle = file.openSync();
      try {
        final length = handle.lengthSync();
        expect(length, greaterThan(4000000));
        var readBytes = 0;
        var calls = 0;
        final result = readLastMatchingJsonlLine(
          length: length,
          maxScanBytes: 4000000,
          readAt: (offset, count) {
            calls++;
            handle.setPositionSync(offset);
            final bytes = handle.readSync(count);
            readBytes += bytes.length;
            return bytes;
          },
          extract: _assistant,
        );
        expect(result, '完成驗證');
        expect(calls, 1);
        expect(readBytes, 16 * 1024);
      } finally {
        handle.closeSync();
      }
    },
  );

  test('matches previous tail semantics across UTF-8, CRLF and block boundaries', () {
    final contents = [
      '',
      '\n\n',
      '{"role":"assistant","content":"older"}\n',
      'bad record\n{"role":"assistant","content":"繁體中文😀"}\r\n{"role":"tool"}',
      '{"role":"assistant","content":"${'中文😀' * 100}"}\n{"role":"tool","content":"${'x' * 123}"}\n',
      '{"role":"assistant","content":"older"}\n{"role":"assistant","content":"latest"}',
      '{"role":"assistant","content":"older"}\n{"role":"assistant","content":"unfinished',
    ];
    for (final content in contents) {
      final bytes = Uint8List.fromList(utf8.encode(content));
      for (final limit in [1, 7, 35, 64, 100, 4096]) {
        for (final blockBytes in [1, 3, 7, 16, 31, 128]) {
          final result = readLastMatchingJsonlLine(
            length: bytes.length,
            maxScanBytes: limit,
            blockBytes: blockBytes,
            readAt: (offset, count) =>
                Uint8List.sublistView(bytes, offset, offset + count),
            extract: _assistant,
          );
          expect(
            result,
            _baseline(bytes, limit),
            reason: 'limit=$limit block=$blockBytes',
          );
        }
      }
    }
  });

  test('no matching record never reads beyond the tail budget', () {
    final bytes = Uint8List.fromList(utf8.encode('{"role":"tool"}\n' * 1000));
    var readBytes = 0;
    final result = readLastMatchingJsonlLine(
      length: bytes.length,
      maxScanBytes: 1023,
      blockBytes: 64,
      readAt: (offset, count) {
        expect(offset, greaterThanOrEqualTo(bytes.length - 1023));
        readBytes += count;
        return Uint8List.sublistView(bytes, offset, offset + count);
      },
      extract: _assistant,
    );
    expect(result, isNull);
    expect(readBytes, 1023);
  });

  test('short read after file truncation does not return stale record', () {
    final result = readLastMatchingJsonlLine(
      length: 1024,
      maxScanBytes: 1024,
      readAt: (_, _) => Uint8List(0),
      extract: _assistant,
    );
    expect(result, isNull);
  });
}
