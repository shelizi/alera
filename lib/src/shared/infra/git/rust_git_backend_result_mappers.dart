part of 'rust_git_backend.dart';

extension on RustGitBackend {
  GitChangeEntry _toChangeEntry(rust.GitChangeEntry entry) {
    return GitChangeEntry(
      path: entry.path,
      oldPath: entry.oldPath,
      area: _toArea(entry.area),
      status: _toStatus(entry.status),
      added: entry.added,
      removed: entry.removed,
      isBinary: entry.isBinary,
      isLarge: entry.isLarge,
      submodule: entry.submodule == null
          ? null
          : GitSubmoduleStatus(
              commitChanged: entry.submodule!.commitChanged,
              trackedChanges: entry.submodule!.trackedChanges,
              untrackedChanges: entry.submodule!.untrackedChanges,
              inspectable: entry.submodule!.inspectable,
            ),
    );
  }

  Future<GitStatusResult> _toStatusResult(rust.GitStatusResult result) async {
    final entries = <GitChangeEntry>[];
    for (var index = 0; index < result.entries.length; index += 1) {
      entries.add(_toChangeEntry(result.entries[index]));
      if ((index + 1) % gitStatusWorkChunkSize == 0) {
        await Future.pause();
      }
    }
    if (result.entries.isNotEmpty) {
      await Future.pause();
    }
    final projectedEntries = List<GitChangeEntry>.unmodifiableOf(entries);
    if (projectedEntries.isNotEmpty) {
      await Future.pause();
    }
    final nativeGroups = result.groups;
    if (nativeGroups.isEmpty && projectedEntries.isNotEmpty) {
      // Backward-compatible fallback for older/mock native results. Production
      // Rust status results provide index-only groups so entries cross FRB once.
      return GitStatusResult(
        entries: projectedEntries,
        groups: await GitChangeGroup.fromEntriesChunked(projectedEntries),
      );
    }

    final groups = <GitChangeGroup>[];
    for (final nativeGroup in nativeGroups) {
      final areaEntries = <GitChangeEntry>[];
      for (var index = 0; index < nativeGroup.entryIndices.length; index += 1) {
        final entryIndex = nativeGroup.entryIndices[index];
        if (entryIndex >= projectedEntries.length) {
          throw StateError(
            'Native Git status group index $entryIndex is outside the '
            '${projectedEntries.length}-entry status result.',
          );
        }
        areaEntries.add(projectedEntries[entryIndex]);
        if ((index + 1) % gitStatusWorkChunkSize == 0) {
          await Future.pause();
        }
      }
      final treeRows = <GitChangeTreeRow>[];
      for (var index = 0; index < nativeGroup.treeRows.length; index += 1) {
        final nativeRow = nativeGroup.treeRows[index];
        final entryIndex = nativeRow.entryIndex;
        GitChangeEntry? entry;
        if (entryIndex != null) {
          if (entryIndex >= projectedEntries.length) {
            throw StateError(
              'Native Git status tree index $entryIndex is outside the '
              '${projectedEntries.length}-entry status result.',
            );
          }
          entry = projectedEntries[entryIndex];
        }
        treeRows.add(
          GitChangeTreeRow(
            kind: _toTreeRowKind(nativeRow.kind),
            name: nativeRow.name,
            path: nativeRow.path,
            depth: nativeRow.depth,
            fileCount: nativeRow.fileCount,
            entry: entry,
            entryIndex: entryIndex,
          ),
        );
        if ((index + 1) % gitStatusWorkChunkSize == 0) {
          await Future.pause();
        }
      }
      groups.add(
        GitChangeGroup(
          area: _toArea(nativeGroup.area),
          entries: List<GitChangeEntry>.unmodifiableOf(areaEntries),
          treeRows: List<GitChangeTreeRow>.unmodifiableOf(treeRows),
          entryIndices: List<int>.unmodifiableOf(nativeGroup.entryIndices),
        ),
      );
    }

    // Native code owns grouping, ordering and tree projection. Dart only maps
    // flat-entry indices back to the already-converted entry instances.
    return GitStatusResult(
      entries: projectedEntries,
      groups: List<GitChangeGroup>.unmodifiableOf(groups),
    );
  }

  GitHistoryResult _toHistoryResult(rust.GitHistoryResult result) {
    return GitHistoryResult(
      items: result.items.map(_toHistoryItem).toList(growable: false),
      currentRef: result.currentRef == null
          ? null
          : _toHistoryItemRef(result.currentRef!),
      remoteRef: result.remoteRef == null
          ? null
          : _toHistoryItemRef(result.remoteRef!),
      baseRef: result.baseRef == null
          ? null
          : _toHistoryItemRef(result.baseRef!),
      mergeBase: result.mergeBase,
      hasIncomingChanges: result.hasIncomingChanges,
      hasOutgoingChanges: result.hasOutgoingChanges,
      hasMore: result.hasMore,
      limit: result.limit,
    );
  }

  GitHistoryItem _toHistoryItem(rust.GitHistoryItem item) {
    final timestamp = item.timestamp;
    return GitHistoryItem(
      id: item.id,
      parentIds: item.parentIds,
      subject: item.subject,
      message: item.message,
      displayId: item.displayId,
      author: item.author,
      authorEmail: item.authorEmail,
      timestamp: timestamp == null
          ? null
          : DateTime.fromMillisecondsSinceEpoch(timestamp, isUtc: true),
      references: item.references
          .map(_toHistoryItemRef)
          .toList(growable: false),
    );
  }

  GitHistoryItemRef _toHistoryItemRef(rust.GitHistoryItemRef itemRef) {
    return GitHistoryItemRef(
      id: itemRef.id,
      name: itemRef.name,
      revision: itemRef.revision,
      category: itemRef.category == null
          ? null
          : _toHistoryRefCategory(itemRef.category!),
    );
  }

  GitCommitCompareResult _toCommitCompareResult(
    rust.GitCommitCompareResult result,
  ) {
    return GitCommitCompareResult(
      summary: _toCommitCompareSummary(result.summary),
      entries: result.entries.map(_toCommitChangeEntry).toList(growable: false),
    );
  }

  GitCommitCompareSummary _toCommitCompareSummary(
    rust.GitCommitCompareSummary summary,
  ) {
    return GitCommitCompareSummary(
      commitOid: summary.commitOid,
      parentOid: summary.parentOid,
      compareRef: summary.compareRef,
      baseRef: summary.baseRef,
      changedFiles: summary.changedFiles,
      status: _toCommitCompareStatus(summary.status),
      errorMessage: summary.errorMessage,
    );
  }

  GitCommitChangeEntry _toCommitChangeEntry(rust.GitCommitChangeEntry entry) {
    return GitCommitChangeEntry(
      path: entry.path,
      oldPath: entry.oldPath,
      status: _toStatus(entry.status),
      added: entry.added,
      removed: entry.removed,
    );
  }
}
