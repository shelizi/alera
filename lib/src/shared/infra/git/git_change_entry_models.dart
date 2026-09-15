part of 'git_diff_models.dart';

class const GitChangeEntry({
  required final String path,
  required final GitChangeArea area,
  required final GitChangeStatus status,
  final String? oldPath,
  final int? added,
  final int? removed,
  final bool isBinary = false,
  final bool isLarge = false,
  final GitSubmoduleStatus? submodule,
  final String? submoduleRoot,
}) {
  String get id => '${area.key}::$path';

  bool get isSubmoduleChild => submoduleRoot != null;

  bool get isExpandableSubmodule =>
      submodule != null &&
      !isSubmoduleChild &&
      (submodule!.commitChanged ||
          submodule!.trackedChanges ||
          submodule!.untrackedChanges) &&
      submodule!.inspectable;

  bool get isSubmoduleWorktreeOnly =>
      area == GitChangeArea.unstaged &&
      submodule != null &&
      !submodule!.commitChanged;

  bool get canStageFromParent =>
      !isSubmoduleChild &&
      area != GitChangeArea.staged &&
      !isSubmoduleWorktreeOnly;

  bool get canUnstageFromParent =>
      !isSubmoduleChild && area == GitChangeArea.staged;

  bool get canDiscardFromParent =>
      !isSubmoduleChild &&
      area != GitChangeArea.staged &&
      !isSubmoduleWorktreeOnly &&
      (submodule == null ||
          (submodule!.inspectable &&
              !submodule!.trackedChanges &&
              !submodule!.untrackedChanges));

  GitChangeEntry insideSubmodule(String root) {
    final prefixedOldPath = oldPath == null ? null : '$root/$oldPath';
    return GitChangeEntry(
      path: '$root/$path',
      oldPath: prefixedOldPath,
      area: area,
      status: status,
      added: added,
      removed: removed,
      isBinary: isBinary,
      isLarge: isLarge,
      submodule: submodule,
      submoduleRoot: root,
    );
  }
}

class const GitSubmoduleStatus({
  required final bool commitChanged,
  required final bool trackedChanges,
  required final bool untrackedChanges,
  required final bool inspectable,
});

typedef _GitChangeEntryValueKey = ({
  String path,
  String? oldPath,
  GitChangeArea area,
  GitChangeStatus status,
  int? added,
  int? removed,
  bool isBinary,
  bool isLarge,
  bool? submoduleCommitChanged,
  bool? submoduleTrackedChanges,
  bool? submoduleUntrackedChanges,
  bool? submoduleInspectable,
  String? submoduleRoot,
});

bool gitSubmoduleStatusValuesEqual(
  GitSubmoduleStatus? left,
  GitSubmoduleStatus? right,
) {
  if (identical(left, right)) {
    return true;
  }
  if (left == null || right == null) {
    return false;
  }
  return left.commitChanged == right.commitChanged &&
      left.trackedChanges == right.trackedChanges &&
      left.untrackedChanges == right.untrackedChanges &&
      left.inspectable == right.inspectable;
}

bool gitChangeEntryValuesEqual(GitChangeEntry left, GitChangeEntry right) {
  if (identical(left, right)) {
    return true;
  }
  return left.path == right.path &&
      left.oldPath == right.oldPath &&
      left.area == right.area &&
      left.status == right.status &&
      left.added == right.added &&
      left.removed == right.removed &&
      left.isBinary == right.isBinary &&
      left.isLarge == right.isLarge &&
      gitSubmoduleStatusValuesEqual(left.submodule, right.submodule) &&
      left.submoduleRoot == right.submoduleRoot;
}

List<GitChangeEntry> reconcileGitChangeEntryInstances(
  List<GitChangeEntry> previous,
  List<GitChangeEntry> next,
) {
  if (identical(previous, next)) {
    return previous;
  }

  List<GitChangeEntry> merged;
  List<bool>? reusedPrevious;
  List<bool>? reusedNext;
  var reusedPrefixLength = 0;

  // An unchanged refresh is the dominant case. Scan it allocation-free and
  // only materialize merge bookkeeping after the first positional mismatch.
  if (previous.length == next.length) {
    List<GitChangeEntry>? positionalMerged;
    for (var index = 0; index < next.length; index += 1) {
      if (gitChangeEntryValuesEqual(previous[index], next[index])) {
        if (positionalMerged != null) {
          positionalMerged[index] = previous[index];
          reusedPrevious![index] = true;
          reusedNext![index] = true;
        }
        continue;
      }
      if (positionalMerged == null) {
        reusedPrefixLength = index;
        positionalMerged = List<GitChangeEntry>.of(next);
        reusedPrevious = List<bool>.filled(previous.length, false);
        reusedNext = List<bool>.filled(next.length, false);
      }
    }
    if (positionalMerged == null) {
      return previous;
    }
    positionalMerged.setRange(0, reusedPrefixLength, previous);
    merged = positionalMerged;
  } else {
    merged = List<GitChangeEntry>.of(next);
  }

  final previousByValue = <_GitChangeEntryValueKey, List<GitChangeEntry>>{};
  for (var index = 0; index < previous.length; index += 1) {
    if (index < reusedPrefixLength || (reusedPrevious?[index] ?? false)) {
      continue;
    }
    final entry = previous[index];
    (previousByValue[_gitChangeEntryValueKey(entry)] ??= <GitChangeEntry>[])
        .add(entry);
  }

  for (var index = 0; index < next.length; index += 1) {
    if (index < reusedPrefixLength || (reusedNext?[index] ?? false)) {
      continue;
    }
    final candidates = previousByValue[_gitChangeEntryValueKey(next[index])];
    if (candidates != null && candidates.isNotEmpty) {
      merged[index] = candidates.removeLast();
    }
  }

  return List<GitChangeEntry>.unmodifiableOf(merged);
}

Future<List<GitChangeEntry>> reconcileGitChangeEntryInstancesChunked(
  List<GitChangeEntry> previous,
  List<GitChangeEntry> next, {
  int chunkSize = gitStatusWorkChunkSize,
  void Function(double milliseconds)? onChunk,
}) async {
  _validateGitStatusChunkSize(chunkSize);
  if (identical(previous, next)) {
    return previous;
  }

  final chunker = _GitStatusChunker(onChunk, chunkSize);
  try {
    List<GitChangeEntry> merged;
    List<bool>? reusedPrevious;
    List<bool>? reusedNext;
    var reusedPrefixLength = 0;

    // Keep the unchanged refresh path allocation-free while preserving the
    // existing chunk-yield cadence for large status lists.
    if (previous.length == next.length) {
      List<GitChangeEntry>? positionalMerged;
      for (var index = 0; index < next.length; index += 1) {
        if (gitChangeEntryValuesEqual(previous[index], next[index])) {
          if (positionalMerged != null) {
            positionalMerged[index] = previous[index];
            reusedPrevious![index] = true;
            reusedNext![index] = true;
          }
        } else if (positionalMerged == null) {
          reusedPrefixLength = index;
          positionalMerged = List<GitChangeEntry>.of(next);
          reusedPrevious = List<bool>.filled(previous.length, false);
          reusedNext = List<bool>.filled(next.length, false);
        }
        if ((index + 1) % chunkSize == 0) {
          await chunker.pause();
        }
      }
      if (positionalMerged == null) {
        return previous;
      }
      positionalMerged.setRange(0, reusedPrefixLength, previous);
      merged = positionalMerged;
    } else {
      merged = List<GitChangeEntry>.of(next);
    }

    final previousByValue = <_GitChangeEntryValueKey, List<GitChangeEntry>>{};
    for (var index = 0; index < previous.length; index += 1) {
      if (index >= reusedPrefixLength && !(reusedPrevious?[index] ?? false)) {
        final entry = previous[index];
        (previousByValue[_gitChangeEntryValueKey(entry)] ??= <GitChangeEntry>[])
            .add(entry);
      }
      if ((index + 1) % chunkSize == 0) {
        await chunker.pause();
      }
    }

    for (var index = 0; index < next.length; index += 1) {
      if (index >= reusedPrefixLength && !(reusedNext?[index] ?? false)) {
        final candidates =
            previousByValue[_gitChangeEntryValueKey(next[index])];
        if (candidates != null && candidates.isNotEmpty) {
          merged[index] = candidates.removeLast();
        }
      }
      if ((index + 1) % chunkSize == 0) {
        await chunker.pause();
      }
    }

    return List<GitChangeEntry>.unmodifiableOf(merged);
  } finally {
    chunker.finish();
  }
}

bool gitRepositoryStateValuesEqual(
  GitRepositoryState left,
  GitRepositoryState right,
) {
  if (identical(left, right)) {
    return true;
  }
  return left.branch == right.branch &&
      left.upstream == right.upstream &&
      left.ahead == right.ahead &&
      left.behind == right.behind &&
      left.hasConflicts == right.hasConflicts &&
      left.headMessage == right.headMessage;
}

bool gitStashEntryValuesEqual(GitStashEntry left, GitStashEntry right) {
  if (identical(left, right)) {
    return true;
  }
  return left.index == right.index &&
      left.reference == right.reference &&
      left.message == right.message &&
      left.oid == right.oid;
}

bool gitStashEntriesValuesEqual(
  List<GitStashEntry> left,
  List<GitStashEntry> right,
) {
  if (identical(left, right)) {
    return true;
  }
  if (left.length != right.length) {
    return false;
  }
  for (var index = 0; index < left.length; index += 1) {
    if (!gitStashEntryValuesEqual(left[index], right[index])) {
      return false;
    }
  }
  return true;
}

_GitChangeEntryValueKey _gitChangeEntryValueKey(GitChangeEntry entry) {
  final submodule = entry.submodule;
  return (
    path: entry.path,
    oldPath: entry.oldPath,
    area: entry.area,
    status: entry.status,
    added: entry.added,
    removed: entry.removed,
    isBinary: entry.isBinary,
    isLarge: entry.isLarge,
    submoduleCommitChanged: submodule?.commitChanged,
    submoduleTrackedChanges: submodule?.trackedChanges,
    submoduleUntrackedChanges: submodule?.untrackedChanges,
    submoduleInspectable: submodule?.inspectable,
    submoduleRoot: entry.submoduleRoot,
  );
}
