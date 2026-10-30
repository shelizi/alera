part of 'file_workbench_terminal_transfer_journal.dart';

/// Non-mutating diagnostic tools; these never authorize a Journal unlock.
extension FileWorkbenchTerminalTransferJournalInspection
    on FileWorkbenchTerminalTransferJournal {
  /// Enumerate pending / interrupted transactions without invoking the
  /// fail-closed loadPending() that intentionally stops at the first lock.
  /// Each item is a *non-atomic diagnostic observation*; concurrent Apps can
  /// change files immediately after a read. This method NEVER unlocks,
  /// deletes, replays or declares an original Host mutation resolved.
  Future<List<TerminalTransferJournalInventoryItem>>
  inspectRecoveryInventory() async {
    final folder = await _folder();
    if (!await folder.exists()) {
      return const <TerminalTransferJournalInventoryItem>[];
    }
    final fileTypes = <String, Set<String>>{};
    final unexpected = <String, Set<String>>{};
    final validDigest = RegExp(r'^[a-f0-9]{64}$');
    await for (final entity in folder.list(followLinks: false)) {
      final name = p.basename(entity.path);
      final suffix = p.extension(name);
      if (suffix != '.json' && suffix != '.lock') {
        // loadPending() now blocks every unexpected directory entry. The
        // read-only inventory must also *show* that blocker (including a
        // no-extension file) so the operator can investigate the exact name.
        unexpected
            .putIfAbsent('unexpected:$name', () => <String>{})
            .add(suffix);
        continue;
      }
      if (entity is! File) {
        // Symlinks/directories with journal-shaped names are not legitimate
        // pending records. Flag them without following their targets.
        unexpected.putIfAbsent('non-file:$name', () => <String>{}).add(suffix);
        continue;
      }
      final key = p.basenameWithoutExtension(name);
      (validDigest.hasMatch(key) ? fileTypes : unexpected)
          .putIfAbsent(key, () => <String>{})
          .add(suffix);
    }
    final result = <TerminalTransferJournalInventoryItem>[];
    final hashes = fileTypes.keys.toList()..sort();
    for (final hash in hashes) {
      final types = fileTypes[hash]!;
      final pendingPresent = types.contains('.json');
      final lockPresent = types.contains('.lock');
      final pendingFile = File(p.join(folder.path, '$hash.json'));
      final lockFile = File(p.join(folder.path, '$hash.lock'));
      final lockEvidence = lockPresent
          ? await _inspectLockWithHash(lockFile, hash)
          : (valid: false, ownerPid: null, created: null);
      WorkbenchTerminalTabTransferPlan? plan;
      if (pendingPresent) {
        try {
          final decoded = _decode(
            await pendingFile.readAsString(),
            pendingFile.path,
          );
          if (sha256.convert(utf8.encode(decoded.tabId)).toString() == hash) {
            plan = decoded;
          }
        } on Object {
          // Corrupt payload remains visible by hash; never skip or repair it.
        }
      }
      var localConfirmedArchive = false;
      if (plan != null) {
        try {
          final archive = await _resolutionFor(plan);
          if (await FileSystemEntity.type(archive.path, followLinks: false) ==
              FileSystemEntityType.file) {
            localConfirmedArchive = _samePlan(
              await _readResolution(archive),
              plan,
            );
          }
        } on Object {
          // An unsafe root, symlink, or corrupt archive cannot prove success.
        }
      }
      result.add(
        TerminalTransferJournalInventoryItem(
          fileKey: hash,
          tabId: plan?.tabId,
          pendingMutationId: plan?.clientMutationId,
          pendingPresent: pendingPresent,
          pendingReadable: plan != null,
          lockPresent: lockPresent,
          localConfirmedArchive: localConfirmedArchive,
          lockMetadataValid: lockEvidence.valid,
          lockOwnerProcessId: lockEvidence.ownerPid,
          lockCreatedAtUtc: lockEvidence.created,
          unknownFileName: false,
        ),
      );
    }
    final unknownKeys = unexpected.keys.toList()..sort();
    for (final key in unknownKeys) {
      final types = unexpected[key]!;
      result.add(
        TerminalTransferJournalInventoryItem(
          fileKey: key,
          tabId: null,
          pendingPresent: types.contains('.json'),
          pendingReadable: false,
          lockPresent: types.contains('.lock'),
          localConfirmedArchive: false,
          lockMetadataValid: false,
          unknownFileName: true,
        ),
      );
    }
    return result;
  }

  /// Surface orphan/corrupt resolution archives even when the pending folder
  /// is empty. A crash after archive creation may leave no corresponding
  /// pending JSON, but neither this method nor a matching local archive can
  /// confirm Host completion. Read-only; never follows links or clears files.
  Future<List<TerminalTransferJournalResolutionInventoryItem>>
  inspectResolutionArchiveInventory() async {
    final support = await _supportDirectory();
    final folder = Directory(
      p.join(support.path, 'terminal_transfer_resolved_v1'),
    );
    await _requireRealDirectoryOrAbsent(folder);
    if (!await folder.exists()) {
      return const <TerminalTransferJournalResolutionInventoryItem>[];
    }
    final entries = <TerminalTransferJournalResolutionInventoryItem>[];
    final validName = RegExp(r'^[a-f0-9]{64}\.json$');
    await for (final entity in folder.list(followLinks: false)) {
      final fileName = p.basename(entity.path);
      final isFile =
          entity is File &&
          await FileSystemEntity.type(entity.path, followLinks: false) ==
              FileSystemEntityType.file;
      WorkbenchTerminalTabTransferPlan? plan;
      var matchesFileName = false;
      if (isFile && validName.hasMatch(fileName)) {
        try {
          final resolved = await _readResolution(File(entity.path));
          final digest = sha256.convert(
            utf8.encode(
              jsonEncode(<String>[resolved.tabId, resolved.clientMutationId]),
            ),
          );
          matchesFileName = fileName == '$digest.json';
          plan = resolved;
        } on Object {
          // A truncated, malformed or unexpected local audit is not proof.
        }
      }
      entries.add(
        TerminalTransferJournalResolutionInventoryItem(
          fileName: fileName,
          ordinaryFile: isFile,
          localArchiveWellFormed: plan != null,
          fileNameMatchesMutation: matchesFileName,
          tabId: plan?.tabId,
          clientMutationId: plan?.clientMutationId,
        ),
      );
    }
    entries.sort((first, second) => first.fileName.compareTo(second.fileName));
    return entries;
  }

  /// Combines the entire pending/lock directory with resolved archives for
  /// a human review, including orphan archives when there are no pending
  /// transfers. Never creates/deletes files or infers that a process died.
  /// If either evidence root is unsafe, propagate failure: a partial
  /// dossier must not be confused with an empty, successfully scanned one.
  Future<TerminalTransferJournalManualRecoveryDossier>
  inspectManualRecoveryDossier() async {
    final observedAtUtc = DateTime.now().toUtc();
    final pendingAndLocks = await inspectRecoveryInventory();
    final localResolutionArchives = await inspectResolutionArchiveInventory();
    return TerminalTransferJournalManualRecoveryDossier(
      observedAtUtc: observedAtUtc,
      pendingAndLocks: List.unmodifiable(pendingAndLocks),
      localResolutionArchives: List.unmodifiable(localResolutionArchives),
    );
  }
}
