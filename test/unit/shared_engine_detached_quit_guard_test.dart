import 'dart:async';

import 'package:alera/src/features/workbench/application/shared_engine_detached_quit_guard.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'normal quit with no detached dirty Editor proceeds without prompt',
    () async {
      var prompts = 0;
      final result = await const SharedEngineDetachedQuitGuard().confirmAndSave(
        tabIds: const ['editor-a'],
        isDirty: (_) => false,
        confirm: (_) async {
          prompts++;
          return false;
        },
        save: (_) async => fail('clean editor should not be saved'),
      );
      expect(result, isTrue);
      expect(prompts, 0);
    },
  );

  test('cancelling quit leaves dirty child untouched and app alive', () async {
    var saves = 0;
    final result = await const SharedEngineDetachedQuitGuard().confirmAndSave(
      tabIds: const ['editor-a'],
      isDirty: (_) => true,
      confirm: (ids) async {
        expect(ids, ['editor-a']);
        return false;
      },
      save: (_) async => saves++,
    );
    expect(result, isFalse);
    expect(saves, 0);
  });

  test(
    'saving a dirty child permits quit only after disk write completes',
    () async {
      final dirty = <String>{'editor-a'};
      final saveGate = Completer<void>();
      var saving = false;
      final quitting = const SharedEngineDetachedQuitGuard().confirmAndSave(
        tabIds: const ['editor-a'],
        isDirty: dirty.contains,
        confirm: (_) async => true,
        save: (id) async {
          saving = true;
          await saveGate.future;
          dirty.remove(id);
        },
      );
      await Future<void>.delayed(Duration.zero);
      expect(saving, isTrue);
      saveGate.complete();
      expect(await quitting, isTrue);
      expect(dirty, isEmpty);
    },
  );

  test(
    'failed or incomplete save forbids closing shared Engine with dirty Editor',
    () async {
      final errors = <Object>[];
      final result = await const SharedEngineDetachedQuitGuard().confirmAndSave(
        tabIds: const ['editor-a', 'editor-b'],
        isDirty: (id) => true,
        confirm: (_) async => true,
        save: (id) async {
          if (id == 'editor-a') throw StateError('write failed');
        },
        onSaveFailure: errors.add,
      );
      expect(result, isFalse);
      expect(errors, hasLength(1));
    },
  );

  test('saving without actually clearing dirty buffer forbids quit', () async {
    final errors = <Object>[];
    final result = await const SharedEngineDetachedQuitGuard().confirmAndSave(
      tabIds: const ['editor-a'],
      isDirty: (_) => true,
      confirm: (_) async => true,
      save: (_) async {},
      onSaveFailure: errors.add,
    );
    expect(result, isFalse);
    expect(errors, hasLength(1));
  });
}
