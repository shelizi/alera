import 'dart:io';

import 'package:alera/src/features/workbench/application/untitled_draft_store.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('draft survives a new store instance and can be updated', () async {
    final root = await Directory.systemTemp.createTemp('alera-drafts-test-');
    addTearDown(() => root.delete(recursive: true));
    final first = UntitledDraftStore(supportDirectory: () async => root);
    const id = 'tab-123';
    expect(await first.read(id), isNull);
    await first.write(id, 'first\\nline');
    final restarted = UntitledDraftStore(supportDirectory: () async => root);
    expect(await restarted.read(id), 'first\\nline');
    await restarted.write(id, 'second');
    expect(await first.read(id), 'second');
    await restarted.delete(id);
    expect(await restarted.read(id), isNull);
  });

  test('serializes rapid writes and rejects invalid draft IDs', () async {
    final root = await Directory.systemTemp.createTemp('alera-drafts-test-');
    addTearDown(() => root.delete(recursive: true));
    final store = UntitledDraftStore(supportDirectory: () async => root);
    await Future.wait(<Future<void>>[
      store.write('draft-1', 'one'),
      store.write('draft-1', 'two'),
      store.write('draft-1', 'three'),
    ]);
    expect(await store.read('draft-1'), 'three');
    expect(() => store.write('../outside', 'bad'), throwsArgumentError);
  });
}
