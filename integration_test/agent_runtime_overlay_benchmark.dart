// ignore_for_file: avoid_print

import 'dart:async';
import 'dart:io';

import 'package:alera/src/rust/api/agent_runtime_overlay.dart' as native;
import 'package:alera/src/rust/frb_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path/path.dart' as p;

const bool _enabled = bool.fromEnvironment('ALERA_RUN_AGENT_OVERLAY_BENCHMARK');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    if (!_enabled) return;
    final libraryPath = Platform.environment['ALERA_NATIVE_LIBRARY_PATH'];
    await RustLib.init(
      externalLibrary: libraryPath == null
          ? null
          : ExternalLibrary.open(libraryPath),
    );
  });

  test('agent overlay production bridge benchmark', () async {
    for (final scenario in const <(String, int)>[
      ('small', 20),
      ('medium', 500),
      ('large', 2000),
    ]) {
      final firstSamples = <int>[];
      final repeatedSamples = <int>[];
      final heartbeatSamples = <int>[];
      final heartbeatGapSamples = <int>[];
      final rssSamples = <int>[];

      for (var sample = 1; sample <= 5; sample += 1) {
        final root = await Directory.systemTemp.createTemp(
          'alera-overlay-ffi-',
        );
        try {
          final source = Directory(p.join(root.path, 'source'))
            ..createSync(recursive: true);
          for (var index = 0; index < scenario.$2; index += 1) {
            File(
              p.join(
                source.path,
                'file-${index.toString().padLeft(4, '0')}.txt',
              ),
            ).writeAsStringSync('0123456789abcdef');
          }

          final overlayRoot = p.join(root.path, 'overlays');
          final overlayPath = p.join(overlayRoot, 'session');
          final request = native.AgentRuntimeOverlayRequest(
            overlayRoot: overlayRoot,
            overlayPath: overlayPath,
            mirrorPath: null,
            sourcePath: source.path,
            managedSubdirectory: null,
            managedFileNames: const <String>[],
            managedFiles: <native.AgentRuntimeOverlayManagedFile>[
              native.AgentRuntimeOverlayManagedFile(
                path: p.join(overlayPath, 'alera-managed.txt'),
                allowedRoot: overlayPath,
                content: 'managed',
                writeMode: native.AgentRuntimeOverlayWriteMode.replace,
                executable: false,
              ),
            ],
          );

          final first = await _measure(request);
          final repeated = await _measure(request);

          expect(first.result.sourceExists, isTrue);
          expect(repeated.result.sourceExists, isTrue);
          expect(
            first.result.linkedCount + first.result.copiedCount,
            greaterThan(BigInt.zero),
          );
          expect(repeated.result.removedCount, BigInt.zero);
          expect(repeated.result.writtenCount, BigInt.zero);
          expect(repeated.result.linkedCount, BigInt.zero);
          expect(repeated.result.copiedCount, BigInt.zero);
          expect(repeated.result.warnings, isEmpty);
          if (first.result.copiedCount > BigInt.zero) {
            expect(first.result.warnings, isNotEmpty);
          }

          firstSamples.add(first.elapsedUs);
          repeatedSamples.add(repeated.elapsedUs);
          heartbeatSamples.add(first.heartbeatTicks);
          heartbeatGapSamples.add(first.maxHeartbeatGapUs);
          rssSamples.add(first.rssDeltaBytes);

          print(
            'overlay_a3_ffi size=${scenario.$1} files=${scenario.$2} '
            'sample=$sample first_us=${first.elapsedUs} '
            'repeated_unchanged_us=${repeated.elapsedUs} '
            'heartbeat_ticks=${first.heartbeatTicks} '
            'max_heartbeat_gap_us=${first.maxHeartbeatGapUs} '
            'rss_delta_bytes=${first.rssDeltaBytes} '
            'linked=${first.result.linkedCount} '
            'copied=${first.result.copiedCount} '
            'warnings=${first.result.warnings.length}',
          );
        } finally {
          if (root.existsSync()) {
            root.deleteSync(recursive: true);
          }
        }
      }

      firstSamples.sort();
      repeatedSamples.sort();
      heartbeatSamples.sort();
      heartbeatGapSamples.sort();
      rssSamples.sort();
      print(
        'overlay_a3_ffi_summary size=${scenario.$1} files=${scenario.$2} '
        'first_median_us=${_median(firstSamples)} '
        'repeated_unchanged_median_us=${_median(repeatedSamples)} '
        'heartbeat_ticks_median=${_median(heartbeatSamples)} '
        'max_heartbeat_gap_median_us=${_median(heartbeatGapSamples)} '
        'rss_delta_median_bytes=${_median(rssSamples)} '
        'first_samples=$firstSamples repeated_samples=$repeatedSamples',
      );

      if (scenario.$1 == 'large') {
        expect(
          _median(heartbeatSamples),
          greaterThan(0),
          reason:
              'The async FRB overlay call should yield to the Dart event loop '
              'while the large native filesystem workload is running.',
        );
      }
    }
  }, skip: !_enabled);
}

Future<_Measurement> _measure(native.AgentRuntimeOverlayRequest request) async {
  final heartbeat = Stopwatch()..start();
  var lastTickUs = heartbeat.elapsedMicroseconds;
  var heartbeatTicks = 0;
  var maxHeartbeatGapUs = 0;
  final timer = Timer.periodic(const Duration(milliseconds: 2), (_) {
    final nowUs = heartbeat.elapsedMicroseconds;
    final gapUs = nowUs - lastTickUs;
    if (gapUs > maxHeartbeatGapUs) {
      maxHeartbeatGapUs = gapUs;
    }
    lastTickUs = nowUs;
    heartbeatTicks += 1;
  });

  final rssBefore = ProcessInfo.currentRss;
  final elapsed = Stopwatch()..start();
  try {
    final result = await native.prepareAgentRuntimeOverlay(request: request);
    elapsed.stop();
    return _Measurement(
      result: result,
      elapsedUs: elapsed.elapsedMicroseconds,
      heartbeatTicks: heartbeatTicks,
      maxHeartbeatGapUs: maxHeartbeatGapUs,
      rssDeltaBytes: ProcessInfo.currentRss - rssBefore,
    );
  } finally {
    timer.cancel();
    heartbeat.stop();
  }
}

int _median(List<int> values) => values[values.length ~/ 2];

final class _Measurement {
  const _Measurement({
    required this.result,
    required this.elapsedUs,
    required this.heartbeatTicks,
    required this.maxHeartbeatGapUs,
    required this.rssDeltaBytes,
  });

  final native.AgentRuntimeOverlayResult result;
  final int elapsedUs;
  final int heartbeatTicks;
  final int maxHeartbeatGapUs;
  final int rssDeltaBytes;
}
