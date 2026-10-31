part of 'file_workbench_terminal_transfer_journal.dart';

final class _TerminalTransferJournalLock {
  const _TerminalTransferJournalLock(
    this.file,
    this.nonce,
    this.tabIdHash,
    this.processEpoch,
  );

  final File file;
  final String nonce;
  final String tabIdHash;
  final String processEpoch;
}

/// A diagnostic snapshot, never a release permission. A surviving .lock may
/// still belong to an active App and local archives are not Host-signed.
final class TerminalTransferJournalRecoveryInspection {
  const TerminalTransferJournalRecoveryInspection({
    required this.lockPresent,
    required this.pendingPresent,
    required this.pendingReadable,
    required this.localConfirmedArchive,
    this.pendingMutationId,
    this.lockOwnerProcessId,
    this.lockOwnerProcessEpoch,
    this.lockCreatedAtUtc,
    this.lockMetadataValid = false,
  });

  final bool lockPresent;
  final bool pendingPresent;
  final bool pendingReadable;
  final bool localConfirmedArchive;
  final String? pendingMutationId;

  /// PID and timestamp are diagnostic hints, not proof of a live process.
  final int? lockOwnerProcessId;

  /// App-created UUID is a diagnostic incarnation hint, never an OS liveness attestation.
  final String? lockOwnerProcessEpoch;
  final DateTime? lockCreatedAtUtc;
  final bool lockMetadataValid;

  bool get requiresManualRecovery =>
      lockPresent || (pendingPresent && !pendingReadable);
}

/// One read-only inventory row, including journals whose Tab ID cannot be
/// decoded from a damaged pending file. A file hash/PID is not a Host proof or
/// a permission to remove any lock.
final class TerminalTransferJournalInventoryItem {
  const TerminalTransferJournalInventoryItem({
    required this.fileKey,
    required this.lockPresent,
    required this.pendingPresent,
    required this.pendingReadable,
    required this.localConfirmedArchive,
    required this.lockMetadataValid,
    required this.unknownFileName,
    this.tabId,
    this.pendingMutationId,
    this.lockOwnerProcessId,
    this.lockOwnerProcessEpoch,
    this.lockCreatedAtUtc,
  });

  final String fileKey;
  final String? tabId;
  final bool lockPresent;
  final bool pendingPresent;
  final bool pendingReadable;
  final bool localConfirmedArchive;
  final bool lockMetadataValid;
  final bool unknownFileName;
  final String? pendingMutationId;
  final int? lockOwnerProcessId;

  /// App-created UUID is a diagnostic incarnation hint, never an OS liveness attestation.
  final String? lockOwnerProcessEpoch;
  final DateTime? lockCreatedAtUtc;

  bool get requiresManualRecovery =>
      unknownFileName || lockPresent || (pendingPresent && !pendingReadable);
}

/// One *local* resolved-archive artifact. A well-formed file is evidence
/// only that this caller previously wrote a record, not that the Host
/// confirmed a live original PTY or that the pending lock may be unlocked.
final class TerminalTransferJournalResolutionInventoryItem {
  const TerminalTransferJournalResolutionInventoryItem({
    required this.fileName,
    required this.ordinaryFile,
    required this.localArchiveWellFormed,
    required this.fileNameMatchesMutation,
    this.tabId,
    this.clientMutationId,
  });

  final String fileName;
  final bool ordinaryFile;
  final bool localArchiveWellFormed;
  final bool fileNameMatchesMutation;
  final String? tabId;
  final String? clientMutationId;

  bool get needsLocalEvidenceReview =>
      !ordinaryFile || !localArchiveWellFormed || !fileNameMatchesMutation;

  /// Deliberately always true: a local audit is never a Host-signed receipt.
  bool get requiresHostVerification => true;
}

/// A local-only cross-reference for a *readable* Pending mutation and audit
/// archives naming that same Tab. A different mutation might be a legitimate
/// older move, not evidence of Host tampering or authorization to unlock.
final class TerminalTransferJournalLocalCrossReference {
  const TerminalTransferJournalLocalCrossReference({
    required this.tabId,
    required this.pendingMutationId,
    required this.pendingFileKey,
    required this.matchingLocalArchiveNames,
    required this.otherMutationLocalArchiveNames,
  });

  final String tabId;
  final String pendingMutationId;
  final String pendingFileKey;
  final List<String> matchingLocalArchiveNames;
  final List<String> otherMutationLocalArchiveNames;

  bool get hasMatchingLocalArchive => matchingLocalArchiveNames.isNotEmpty;
  bool get hasHistoricalOtherMutationArchives =>
      otherMutationLocalArchiveNames.isNotEmpty;

  /// The local comparison is never a Host-signed transfer outcome.
  bool get requiresHostVerification => true;
}

/// The non-atomic, strictly read-only evidence package an operator can
/// inspect before choosing any *future* manual recovery action. The two
/// inventories may change between observations. This is neither a process
/// liveness assertion nor a Host-signed terminal.transfer outcome.
final class TerminalTransferJournalManualRecoveryDossier {
  const TerminalTransferJournalManualRecoveryDossier({
    required this.observedAtUtc,
    required this.pendingAndLocks,
    required this.localResolutionArchives,
  });

  final DateTime observedAtUtc;
  final List<TerminalTransferJournalInventoryItem> pendingAndLocks;
  final List<TerminalTransferJournalResolutionInventoryItem>
  localResolutionArchives;

  bool get hasPendingOrLockEvidence => pendingAndLocks.any(
    (item) => item.pendingPresent || item.lockPresent || item.unknownFileName,
  );

  bool get hasUnreadableOrUnsafeLocalEvidence =>
      pendingAndLocks.any((item) => item.requiresManualRecovery) ||
      localResolutionArchives.any((item) => item.needsLocalEvidenceReview);

  /// Even a valid-looking archive cannot prove the original Host/PTY state.
  bool get requiresHostVerification => true;

  /// Correlate *only* independently readable local evidence. A malformed
  /// Pending JSON or unreadable archive is preserved in its raw inventory
  /// rather than being guessed into a particular Tab/Mutation.
  List<TerminalTransferJournalLocalCrossReference>
  get readablePendingArchiveCrossReferences {
    final verifiedArchives = localResolutionArchives
        .where(
          (archive) =>
              archive.ordinaryFile &&
              archive.localArchiveWellFormed &&
              archive.fileNameMatchesMutation &&
              archive.tabId != null &&
              archive.clientMutationId != null,
        )
        .toList();
    return List.unmodifiable(
      pendingAndLocks
          .where(
            (pending) =>
                pending.pendingPresent &&
                pending.pendingReadable &&
                pending.tabId != null &&
                pending.pendingMutationId != null,
          )
          .map((pending) {
            final sameTab = verifiedArchives
                .where((archive) => archive.tabId == pending.tabId)
                .toList();
            return TerminalTransferJournalLocalCrossReference(
              tabId: pending.tabId!,
              pendingMutationId: pending.pendingMutationId!,
              pendingFileKey: pending.fileKey,
              matchingLocalArchiveNames: List.unmodifiable(
                sameTab
                    .where(
                      (archive) =>
                          archive.clientMutationId == pending.pendingMutationId,
                    )
                    .map((archive) => archive.fileName),
              ),
              otherMutationLocalArchiveNames: List.unmodifiable(
                sameTab
                    .where(
                      (archive) =>
                          archive.clientMutationId != pending.pendingMutationId,
                    )
                    .map((archive) => archive.fileName),
              ),
            );
          }),
    );
  }

  /// These archives have no corresponding *readable same-mutation Pending*
  /// in this non-atomic scan. They are not necessarily abandoned transactions:
  /// prior resolved operations may legitimately be archived.
  List<String> get archivesWithoutReadableMatchingPending {
    final readablePendingKeys = pendingAndLocks
        .where((pending) => pending.pendingReadable && pending.tabId != null)
        .map((pending) => (pending.tabId!, pending.pendingMutationId))
        .toSet();
    return List.unmodifiable(
      localResolutionArchives
          .where(
            (archive) =>
                archive.ordinaryFile &&
                archive.localArchiveWellFormed &&
                archive.fileNameMatchesMutation &&
                archive.tabId != null &&
                !readablePendingKeys.contains((
                  archive.tabId!,
                  archive.clientMutationId,
                )),
          )
          .map((archive) => archive.fileName),
    );
  }

  /// No local PID, timestamp, archive, or missing file can release the lock.
  bool get permitsAutomaticUnlock => false;
}
