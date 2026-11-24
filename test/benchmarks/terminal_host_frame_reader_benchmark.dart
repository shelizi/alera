// Standalone Dart microbenchmark; no Flutter/native assets required.
// Run the same harness against two revisions of the frame codec.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:alera/src/platform/runtime_host/transport/terminal_host_frame_codec.dart';

List<Uint8List> split(Uint8List bytes, int chunkBytes) => [
  for (var offset = 0; offset < bytes.length; offset += chunkBytes)
    Uint8List.sublistView(
      bytes,
      offset,
      (offset + chunkBytes).clamp(0, bytes.length),
    ),
];

void measure(
  String name,
  List<Uint8List> chunks, {
  required bool binary,
  required int expectedFrames,
  required int expectedUnits,
}) {
  int decode() {
    final reader = TerminalHostFrameReader();
    if (binary) reader.upgradeToBinary();
    var count = 0;
    var units = 0;
    for (final chunk in chunks) {
      for (final frame in reader.add(chunk)) {
        count++;
        units += switch (frame) {
          TerminalHostJsonFrame(:final json) => json.length,
          TerminalHostOutputFrame(:final data) => data.length,
        };
      }
    }
    if (count != expectedFrames || units != expectedUnits) {
      throw StateError('$name: unexpected decoded count or length');
    }
    return units;
  }

  for (var i = 0; i < 3; i++) {
    decode();
  }
  final micros = <int>[];
  for (var i = 0; i < 7; i++) {
    final clock = Stopwatch()..start();
    decode();
    clock.stop();
    micros.add(clock.elapsedMicroseconds);
  }
  micros.sort();
  stdout.writeln(
    jsonEncode({
      'case': name,
      'frames': expectedFrames,
      'chunks': chunks.length,
      'inputBytes': chunks.fold<int>(0, (sum, chunk) => sum + chunk.length),
      'medianUs': micros[micros.length ~/ 2],
      'samplesUs': micros,
    }),
  );
}

void main() {
  const frameCount = 2048;
  final output = Uint8List(128)..fillRange(0, 128, 0x61);
  final burst = BytesBuilder(copy: false);
  for (var i = 0; i < frameCount; i++) {
    burst.add(encodeTerminalHostOutputFrame('session-${i % 8}', output));
  }
  final burstBytes = burst.takeBytes();
  measure(
    'output-64k-chunks',
    split(burstBytes, 64 * 1024),
    binary: true,
    expectedFrames: frameCount,
    expectedUnits: frameCount * output.length,
  );

  final snapshotJson = jsonEncode({'snapshot': 'x' * (1024 * 1024)});
  measure(
    'legacy-json-4k-chunks',
    split(utf8.encode('$snapshotJson\n'), 4 * 1024),
    binary: false,
    expectedFrames: 1,
    expectedUnits: snapshotJson.length,
  );
  measure(
    'binary-json-4k-chunks',
    split(encodeTerminalHostJsonFrame(snapshotJson), 4 * 1024),
    binary: true,
    expectedFrames: 1,
    expectedUnits: snapshotJson.length,
  );
}
