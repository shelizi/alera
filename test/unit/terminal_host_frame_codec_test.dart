import 'dart:convert';
import 'dart:typed_data';

import 'package:alera/src/platform/runtime_host/transport/terminal_host_frame_codec.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('TerminalHostFrameReader', () {
    test('rejects an oversized announced binary frame immediately', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();

      // A corrupted length must not leave the socket reader buffering PTY
      // output indefinitely, even when no payload bytes have arrived.
      expect(
        () => reader.add(<int>[1, 0x7f, 0xff, 0xff, 0xff]),
        throwsA(isA<FormatException>()),
      );
    });

    test('reads newline JSON before the upgrade', () {
      final reader = TerminalHostFrameReader();

      final frames = reader.add(utf8.encode('{"a":1}\n{"b":2}\n'));

      expect(frames, hasLength(2));
      expect((frames[0] as TerminalHostJsonFrame).json, '{"a":1}');
      expect((frames[1] as TerminalHostJsonFrame).json, '{"b":2}');
    });

    test('a line split across chunks is reassembled', () {
      final reader = TerminalHostFrameReader();

      expect(reader.add(utf8.encode('{"a":')), isEmpty);
      final frames = reader.add(utf8.encode('1}\n'));

      expect((frames.single as TerminalHostJsonFrame).json, '{"a":1}');
    });

    test('switches to frames only after the upgrade', () {
      // The handshake response is a line; everything after it is framed. This
      // ordering is what the in-band upgrade on the host guarantees.
      final reader = TerminalHostFrameReader();
      final lines = reader.add(utf8.encode('{"ok":true}\n'));
      expect((lines.single as TerminalHostJsonFrame).json, '{"ok":true}');

      reader.upgradeToBinary();
      final frames = reader.add(
        encodeTerminalHostOutputFrame('session-1', <int>[1, 2, 3]),
      );

      final output = frames.single as TerminalHostOutputFrame;
      expect(output.sessionId, 'session-1');
      expect(output.data, <int>[1, 2, 3]);
    });

    test('an output frame carries raw bytes, including invalid UTF-8', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      final data = <int>[0x1b, 0x5b, 0x30, 0x6d, 0xff, 0x00];

      final frames = reader.add(
        encodeTerminalHostOutputFrame('session-1', data),
      );

      expect((frames.single as TerminalHostOutputFrame).data, data);
    });

    test('a frame split across chunks is reassembled', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      final bytes = encodeTerminalHostOutputFrame('session-1', <int>[7, 8, 9]);

      expect(reader.add(bytes.sublist(0, 3)), isEmpty);
      expect(reader.add(bytes.sublist(3, 8)), isEmpty);
      final frames = reader.add(bytes.sublist(8));

      expect((frames.single as TerminalHostOutputFrame).data, <int>[7, 8, 9]);
    });

    test('several frames in one chunk all come out', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      final bytes = <int>[
        ...encodeTerminalHostOutputFrame('a', <int>[1]),
        ...encodeTerminalHostJsonFrame('{"event":"exit"}'),
        ...encodeTerminalHostOutputFrame('b', <int>[2]),
      ];

      final frames = reader.add(bytes);

      expect(frames, hasLength(3));
      expect((frames[0] as TerminalHostOutputFrame).sessionId, 'a');
      expect((frames[1] as TerminalHostJsonFrame).json, '{"event":"exit"}');
      expect((frames[2] as TerminalHostOutputFrame).sessionId, 'b');
    });

    test('one chunk may contain the sentinel and many binary frames', () {
      final reader = TerminalHostFrameReader();
      final chunk = BytesBuilder(copy: false)
        ..add(utf8.encode('{"event":"binaryFramesEnabled","payload":{}}\n'));
      const count = 2048;
      for (var index = 0; index < count; index++) {
        chunk.add(
          encodeTerminalHostOutputFrame('s${index % 3}', <int>[
            index & 255,
            index >> 8,
            0xff,
          ]),
        );
      }
      final frames = reader.add(chunk.takeBytes());
      expect(reader.isBinary, isTrue);
      expect(frames, hasLength(count + 1));
      for (var index = 0; index < count; index++) {
        final frame = frames[index + 1] as TerminalHostOutputFrame;
        expect(frame.sessionId, 's${index % 3}');
        expect(frame.data, <int>[index & 255, index >> 8, 0xff]);
      }
    });

    test('all split points preserve the upgrade and binary frame order', () {
      final chunk = Uint8List.fromList(<int>[
        ...utf8.encode('{"event":"binaryFramesEnabled"}\n'),
        ...encodeTerminalHostJsonFrame('{"text":"繁體中文😀"}'),
        ...encodeTerminalHostOutputFrame('終端機', <int>[0xff, 0, 7]),
        ...encodeTerminalHostJsonFrame(''),
        ...encodeTerminalHostOutputFrame('b', <int>[]),
      ]);
      for (var split = 0; split <= chunk.length; split++) {
        final reader = TerminalHostFrameReader();
        final frames = <TerminalHostFrame>[
          ...reader.add(Uint8List.sublistView(chunk, 0, split)),
          ...reader.add(Uint8List.sublistView(chunk, split)),
        ];
        expect(frames, hasLength(5), reason: 'split at $split');
        expect((frames[1] as TerminalHostJsonFrame).json, '{"text":"繁體中文😀"}');
        final output = frames[2] as TerminalHostOutputFrame;
        expect(output.sessionId, '終端機');
        expect(output.data, <int>[0xff, 0, 7]);
        expect((frames[3] as TerminalHostJsonFrame).json, '');
        expect((frames[4] as TerminalHostOutputFrame).data, isEmpty);
      }
    });

    test('payload mentions of the sentinel do not upgrade the transport', () {
      final reader = TerminalHostFrameReader();
      final frames = reader.add(
        utf8.encode(
          '{"event":"output","payload":{"text":"binaryFramesEnabled"}}\n'
          '{"id":2,"ok":true}\n',
        ),
      );
      expect(reader.isBinary, isFalse);
      expect(frames, hasLength(2));
      expect((frames.last as TerminalHostJsonFrame).json, '{"id":2,"ok":true}');
    });

    test('skips unknown and malformed output frames without stalling', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      // Unknown kind with a payload split across reads.
      expect(reader.add(<int>[99, 0, 0, 0, 4, 1, 2]), isEmpty);
      final frames = reader.add(<int>[
        3, 4,
        99, 0, 0, 0, 0, // Unknown empty frame.
        2, 0, 0, 0, 1, 0, // Missing output ID length.
        2, 0, 0, 0, 3, 0, 9, 1, // ID longer than the frame.
        ...encodeTerminalHostJsonFrame('{"ok":true}'),
      ]);
      expect((frames.single as TerminalHostJsonFrame).json, '{"ok":true}');
    });

    test('partial payloads and output never alias mutable socket input', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      final wire = encodeTerminalHostOutputFrame('id', <int>[7, 8, 9]);
      final split = wire.length - 1;
      final prefix = wire.sublist(0, split);
      expect(reader.add(prefix), isEmpty);
      prefix.fillRange(0, prefix.length, 0);
      final tail = wire.sublist(split);
      final first = reader.add(tail).single as TerminalHostOutputFrame;
      tail[0] = 0;
      expect(first.data, <int>[7, 8, 9]);

      final second = reader.add(wire).single as TerminalHostOutputFrame;
      wire.fillRange(0, wire.length, 0);
      expect(second.data, <int>[7, 8, 9]);
      second.data[0] = 99;
      expect(first.data, <int>[7, 8, 9]);
      expect(second.data.buffer.lengthInBytes, 3);
    });

    test(
      'large fragmented legacy and binary JSON decode without rescanning',
      () {
        final text = '繁體中文😀' * (128 * 1024);
        final json = jsonEncode(<String, Object?>{'snapshot': text});
        for (final binary in <bool>[false, true]) {
          final reader = TerminalHostFrameReader();
          if (binary) reader.upgradeToBinary();
          final wire = binary
              ? encodeTerminalHostJsonFrame(json)
              : utf8.encode('$json\n');
          final frames = <TerminalHostFrame>[];
          const fragmentSize = 997;
          for (var start = 0; start < wire.length; start += fragmentSize) {
            final end = (start + fragmentSize).clamp(0, wire.length);
            frames.addAll(reader.add(Uint8List.sublistView(wire, start, end)));
          }
          expect((frames.single as TerminalHostJsonFrame).json, json);
        }
      },
    );

    test('rejects an oversized header even when fragmented', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();
      expect(reader.add(<int>[1, 0x7f]), isEmpty);
      expect(
        () => reader.add(<int>[0xff, 0xff, 0xff]),
        throwsA(isA<FormatException>()),
      );
    });

    test('empty payloads round-trip', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();

      final frames = reader.add(encodeTerminalHostOutputFrame('', <int>[]));

      final output = frames.single as TerminalHostOutputFrame;
      expect(output.sessionId, isEmpty);
      expect(output.data, isEmpty);
    });

    test('a multi-byte session id survives the byte-counted prefix', () {
      final reader = TerminalHostFrameReader()..upgradeToBinary();

      final frames = reader.add(
        encodeTerminalHostOutputFrame('sesión', <int>[1]),
      );

      expect((frames.single as TerminalHostOutputFrame).sessionId, 'sesión');
    });
  });

  group('cross-implementation contract', () {
    // Hardcoded bytes both sides must agree on. Without this the two codecs
    // can drift apart while each stays self-consistent.
    test('a JSON frame has the exact expected header', () {
      final bytes = encodeTerminalHostJsonFrame('{}');

      expect(bytes, <int>[1, 0, 0, 0, 2, 0x7b, 0x7d]);
    });

    test('an output frame has the exact expected layout', () {
      final bytes = encodeTerminalHostOutputFrame('ab', <int>[0xff]);

      expect(bytes, <int>[
        2, // kind
        0, 0, 0, 5, // payload length: 2 id-length bytes + 2 id + 1 data
        0, 2, // session id length
        0x61, 0x62, // "ab"
        0xff, // raw data
      ]);
    });
  });

  test('encodeTerminalHostOutputFrame returns a typed byte list', () {
    expect(
      encodeTerminalHostOutputFrame('a', <int>[1]),
      isA<Uint8List>(),
      reason: 'the socket writer needs bytes, not a boxed list',
    );
  });
}
