import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/infra/file_workbench_terminal_transfer_journal.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  group('Terminal transfer manual recovery dossier', () {
    Future<void> expectUnsafeLockPreserved({
      required int processId,
      required String createdAtUtc,
      bool validMetadata = false,
    }) async {
      final root = await Directory.systemTemp.createTemp(
        'alera-terminal-transfer-manual-',
      );
      addTearDown(() async {
        if (await root.exists()) await root.delete(recursive: true);
      });
      final folder = Directory(
        p.join(root.path, 'terminal_transfer_pending_v1'),
      );
      await folder.create(recursive: true);
      const tabId = 'interrupted-terminal';
      final hash = sha256.convert(utf8.encode(tabId)).toString();
      final lock = File(p.join(folder.path, '$hash.lock'));
      await lock.writeAsString(
        jsonEncode(<String, Object?>{
          'version': 2,
          'tabIdHash': hash,
          'nonce': 'original-lock-nonce',
          'processId': processId,
          'processEpoch': '00000000-0000-4000-8000-000000000001',
          'createdAtUtc': createdAtUtc,
        }),
        flush: true,
      );

      final journal = FileWorkbenchTerminalTransferJournal(
        supportDirectory: () async => root,
      );
      final dossier = await journal.inspectManualRecoveryDossier();
      final lockEvidence = dossier.pendingAndLocks.single;
      expect(lockEvidence.lockPresent, isTrue);
      expect(lockEvidence.pendingPresent, isFalse);
      expect(lockEvidence.lockMetadataValid, validMetadata);
      expect(
        lockEvidence.lockOwnerProcessEpoch,
        validMetadata ? '00000000-0000-4000-8000-000000000001' : isNull,
      );
      final tabInspection = await journal.inspectRecovery(tabId);
      expect(
        tabInspection.lockOwnerProcessEpoch,
        lockEvidence.lockOwnerProcessEpoch,
      );
      expect(lockEvidence.requiresManualRecovery, isTrue);
      expect(dossier.requiresHostVerification, isTrue);
      expect(dossier.permitsAutomaticUnlock, isFalse);
      await expectLater(journal.loadPending(), throwsStateError);
      expect(await lock.exists(), isTrue);
    }

    test('rejects impossible owner PID without discarding the lock', () async {
      await expectUnsafeLockPreserved(
        processId: -1,
        createdAtUtc: '2026-10-31T01:00:00.000Z',
      );
    });

    test(
      'rejects non-UTC owner timestamps without discarding the lock',
      () async {
        await expectUnsafeLockPreserved(
          processId: 1024,
          createdAtUtc: '2026-10-31T01:00:00',
        );
      },
    );
    test('well-formed lock remains blocked without Host attestation', () async {
      await expectUnsafeLockPreserved(
        processId: 1024,
        createdAtUtc: '2026-10-31T01:00:00.000Z',
        validMetadata: true,
      );
    });
  });
}
