import 'dart:async';

import 'package:code_forge/code_forge/versioned_text_snapshot_cache.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('VersionedTextSnapshotCache', () {
    test(
      'reuses one snapshot for repeated reads of the same version',
      () async {
        final cache = VersionedTextSnapshotCache();
        var loads = 0;

        Future<String> load() async {
          loads++;
          return 'snapshot-$loads';
        }

        expect(await cache.get(version: 7, load: load), 'snapshot-1');
        expect(await cache.get(version: 7, load: load), 'snapshot-1');
        expect(loads, 1);
      },
    );

    test('deduplicates concurrent snapshot loads for one version', () async {
      final cache = VersionedTextSnapshotCache();
      final completer = Completer<String>();
      var loads = 0;

      Future<String> load() {
        loads++;
        return completer.future;
      }

      final first = cache.get(version: 7, load: load);
      final second = cache.get(version: 7, load: load);
      expect(loads, 1);

      completer.complete('snapshot');
      expect(await Future.wait([first, second]), ['snapshot', 'snapshot']);
    });

    test('loads again when the document version changes', () async {
      final cache = VersionedTextSnapshotCache();
      var loads = 0;

      Future<String> load() async => 'snapshot-${++loads}';

      expect(await cache.get(version: 7, load: load), 'snapshot-1');
      expect(await cache.get(version: 8, load: load), 'snapshot-2');
      expect(loads, 2);
    });

    test('invalidate forces a reload for the same version', () async {
      final cache = VersionedTextSnapshotCache();
      var loads = 0;

      Future<String> load() async => 'snapshot-${++loads}';

      expect(await cache.get(version: 7, load: load), 'snapshot-1');
      cache.invalidate();
      expect(await cache.get(version: 7, load: load), 'snapshot-2');
    });

    test('failed loads are not retained', () async {
      final cache = VersionedTextSnapshotCache();
      var loads = 0;

      Future<String> load() async {
        loads++;
        if (loads == 1) throw StateError('snapshot failed');
        return 'recovered';
      }

      await expectLater(
        cache.get(version: 7, load: load),
        throwsA(isA<StateError>()),
      );
      expect(await cache.get(version: 7, load: load), 'recovered');
      expect(loads, 2);
    });
  });
}
