import 'dart:io';
import 'dart:typed_data';

import 'package:code_forge/code_forge/rope.dart' as code_forge;
import 'package:code_forge/src/rust/api/native_save_io.dart' as save;
import 'package:code_forge/src/rust/api/workspace_source.dart' as source;
import 'package:code_forge/src/rust/frb_generated.dart' as rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;
import 'package:flutter_test/flutter_test.dart';

int _percentile(List<int> sorted, double p) =>
    sorted[((sorted.length - 1) * p).round()];

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final dll = Platform.environment['ALERA_CODE_FORGE_TEST_DLL'];
  final enabled =
      Platform.isWindows &&
      dll != null &&
      dll.isNotEmpty &&
      Platform.environment['ALERA_ROPE_FFI_BENCH'] == '1';

  test(
    'Release Windows native Rope Save p50/p95 through Flutter FFI',
    () async {
      await rust.RustLib.init(externalLibrary: ExternalLibrary.open(dll!));
      final dir = Directory.systemTemp.createTempSync('alera-rope-ffi-bench-');
      try {
        final file = File('${dir.path}\\sample.txt');
        const samples = 21;
        for (final mib in <int>[1, 4, 10]) {
          final size = mib * 1024 * 1024;
          final original = Uint8List(size)..fillRange(0, size, 97);
          final current = code_forge.Rope('b' * size);
          final savedTimes = <int>[];
          final copyTimes = <int>[];
          for (var run = 0; run <= samples; run++) {
            // Fixture reset and token retrieval are outside the timed interval.
            file.writeAsBytesSync(original, flush: true);
            final loaded = await code_forge.Rope.openWorkspaceFile(
              workspacePath: dir.path,
              relativePath: 'sample.txt',
              tabSize: 4,
              encoding: source.NativeWorkspaceTextEncoding.utf8,
            );
            final token = loaded.sourceInfo!.contentToken;
            final stopwatch = Stopwatch()..start();
            final result = await current.saveWorkspaceSnapshot(
              workspacePath: dir.path,
              relativePath: 'sample.txt',
              expectedContentToken: token,
              overwriteIfChanged: false,
              tabSize: 4,
              encoding: source.NativeWorkspaceTextEncoding.utf8,
            );
            stopwatch.stop();
            expect(result.status, save.SaveStatus.saved);
            expect(result.size.toInt(), size);
            if (run > 0) savedTimes.add(stopwatch.elapsedMicroseconds);

            // Diagnostic only, not an old-path Save A/B: measures a separate
            // full-document Rope -> Dart String copy across the Rust bridge.
            final copyWatch = Stopwatch()..start();
            final copied = current.getText();
            copyWatch.stop();
            expect(copied.length, size);
            if (run > 0) copyTimes.add(copyWatch.elapsedMicroseconds);
          }
          savedTimes.sort();
          copyTimes.sort();
          stdout.writeln(
            [
              'ROPE_RELEASE_FFI',
              'size_mib=$mib',
              'samples=$samples',
              'save_p50_us=${_percentile(savedTimes, 0.50)}',
              'save_p95_us=${_percentile(savedTimes, 0.95)}',
              'rope_text_copy_p50_us=${_percentile(copyTimes, 0.50)}',
              'rope_text_copy_p95_us=${_percentile(copyTimes, 0.95)}',
            ].join(' '),
          );
        }
      } finally {
        dir.deleteSync(recursive: true);
      }
    },
    skip: enabled ? false : 'requires Windows DLL and ALERA_ROPE_FFI_BENCH=1',
  );
}
