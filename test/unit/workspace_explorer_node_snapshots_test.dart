import 'dart:math';

import 'package:alera/src/features/workbench/application/workspace_explorer_node_snapshots.dart';
import 'package:flutter_test/flutter_test.dart';

void _expectOrderedMap<V extends Object>(
  Map<String, V> actual,
  Map<String, V> expected,
) {
  expect(actual, expected);
  expect(actual.length, expected.length);
  expect(actual.keys.toList(), expected.keys.toList());
  expect(actual.values.toList(), expected.values.toList());
  expect(
    actual.entries.map((entry) => (entry.key, entry.value)).toList(),
    expected.entries.map((entry) => (entry.key, entry.value)).toList(),
  );
}

void main() {
  test('sparse publications preserve insertion order, identities and old snapshots', () {
    final base = {for (var i = 0; i < 1000; i++) 'node-$i': Object()};
    final store = WorkspaceExplorerNodeSnapshots<Object>()
      ..replace(base.entries);
    final first = store.snapshot();
    expect(store.snapshot(), same(first));
    final expected = Map<String, Object>.of(base);
    void put(String id) {
      final value = Object();
      expected[id] = value;
      store.put(id, value);
    }

    void remove(String id) {
      expected.remove(id);
      store.remove(id);
    }

    remove('node-2');
    put('new-1');
    put('node-2');
    put('node-7');
    remove('new-1');
    put('new-2');
    put('new-1');
    final second = store.snapshot();
    _expectOrderedMap(second, expected);
    expect(second['node-999'], same(first['node-999']));
    expect(first.keys.toList(), base.keys.toList());
    expect(first['node-2'], same(base['node-2']));
    expect(first.containsKey('new-1'), isFalse);
    // Map lookup accepts Object?; deliberately test unrelated keys.
    // ignore: collection_methods_unrelated_type
    expect(second[123], isNull);
    // ignore: collection_methods_unrelated_type
    expect(second.containsKey(null), isFalse);
    store.remove('missing');
    expect(store.snapshot(), same(second));
    final secondReference = Map<String, Object>.of(expected);
    remove('node-2');
    put('node-2');
    final third = store.snapshot();
    _expectOrderedMap(third, expected);
    _expectOrderedMap(second, secondReference);
    store.clear();
    expect(store.snapshot(), isEmpty);
    _expectOrderedMap(first, base);
    _expectOrderedMap(second, secondReference);
    _expectOrderedMap(third, expected);
  });

  test('published overlay rejects every mutation entry point', () {
    final store = WorkspaceExplorerNodeSnapshots<int>()
      ..replace({for (var i = 0; i < 500; i++) '$i': i}.entries)
      ..put('new', 501);
    final map = store.snapshot();
    for (final mutate in <void Function()>[
      () => map['0'] = 10,
      () => map.remove('missing'),
      map.clear,
      () => map.addAll({}),
      () => map.addEntries([]),
      () => map.putIfAbsent('0', () => 99),
      () => map.update('0', (value) => value + 1),
      () => map.updateAll((key, value) => value + 1),
      () => map.removeWhere((key, value) => false),
    ]) {
      expect(mutate, throwsUnsupportedError);
    }
    expect(map['0'], 0);
    expect(map['new'], 501);
    expect(map.length, 501);
  });

  test(
    'random mixed edits, full replacements and compaction match ordered maps',
    () {
      final random = Random(41027);
      final store = WorkspaceExplorerNodeSnapshots<int>();
      var reference = {for (var i = 0; i < 1800; i++) 'node-$i': i};
      store.replace(reference.entries);
      final retained = <(Map<String, int>, Map<String, int>)>[];
      for (var batch = 0; batch < 70; batch++) {
        if (batch == 35) {
          reference = {for (var i = 0; i < 1500; i++) 'node-$i': -i};
          store.replace(reference.entries);
        }
        final operations = batch % 14 == 0 ? 700 : 12;
        for (var i = 0; i < operations; i++) {
          final key = 'node-${random.nextInt(2500)}';
          if (random.nextBool()) {
            final value = random.nextInt(100000);
            reference[key] = value;
            store.put(key, value);
          } else {
            reference.remove(key);
            store.remove(key);
          }
        }
        final snapshot = store.snapshot();
        _expectOrderedMap(snapshot, reference);
        for (var i = 0; i < 20; i++) {
          final key = 'node-${random.nextInt(2500)}';
          expect(snapshot[key], reference[key]);
          expect(snapshot.containsKey(key), reference.containsKey(key));
        }
        if (batch % 7 == 0) {
          retained.add((snapshot, Map<String, int>.of(reference)));
        }
      }
      store.clear();
      for (final (snapshot, expected) in retained) {
        _expectOrderedMap(snapshot, expected);
      }
    },
  );
}
