import 'dart:convert';
import 'dart:typed_data';

import 'package:code_forge/LSP/lsp_message_framer.dart';
import 'package:flutter_test/flutter_test.dart';

List<int> _frame(Object message) {
  final body = utf8.encode(jsonEncode(message));
  return <int>[
    ...ascii.encode('Content-Length: ${body.length}\r\n\r\n'),
    ...body,
  ];
}

List<Object?> _decodeAll(List<Uint8List> bodies) =>
    bodies.map((body) => jsonDecode(utf8.decode(body))).toList();

void main() {
  test('splits several messages delivered in one chunk', () {
    final framer = LspMessageFramer();
    final bodies = framer.add(<int>[
      ..._frame(<String, Object>{'id': 1}),
      ..._frame(<String, Object>{'id': 2}),
    ]);

    expect(_decodeAll(bodies), <Object>[
      <String, Object>{'id': 1},
      <String, Object>{'id': 2},
    ]);
  });

  test('reassembles a message split byte by byte, including a multi-byte character', () {
    final framer = LspMessageFramer();
    final bytes = _frame(<String, Object>{'message': 'héllo 語'});
    final bodies = <Uint8List>[];
    for (final byte in bytes) {
      bodies.addAll(framer.add(<int>[byte]));
    }

    expect(_decodeAll(bodies), <Object>[
      <String, Object>{'message': 'héllo 語'},
    ]);
  });

  test('keeps a trailing partial message across buffer growth', () {
    final framer = LspMessageFramer();
    final large = 'x' * (200 * 1024);
    final first = _frame(<String, Object>{'id': 1});
    final second = _frame(<String, Object>{'payload': large});
    final split = first.length + 10;
    final stream = <int>[...first, ...second];

    final early = framer.add(stream.sublist(0, split));
    final late = framer.add(stream.sublist(split));

    expect(_decodeAll(early), <Object>[
      <String, Object>{'id': 1},
    ]);
    expect(_decodeAll(late), <Object>[
      <String, Object>{'payload': large},
    ]);
  });

  test(
    'decodes large bodies off the calling isolate with the same result',
    () async {
      final payload = <String, Object>{
        'items': List<int>.generate(40000, (i) => i),
      };
      final body = Uint8List.fromList(utf8.encode(jsonEncode(payload)));
      expect(body.length, greaterThan(lspInlineDecodeLimitBytes));

      expect(await decodeLspMessageBody(body), payload);
    },
  );

  test('reports a malformed body as an error instead of a value', () async {
    await expectLater(
      decodeLspMessageBody(Uint8List.fromList(utf8.encode('{not json'))),
      throwsFormatException,
    );
  });
}
