import 'package:alera_mobile/src/core/json_payload_fields.dart';

const int maxMobileWorkspaceFileRangeBytes = 256 * 1024;
const int maxMobileWorkspaceDirectoryPageEntries = 200;

class const MobileWorkspaceTextSaveResult({
  required final String relativePath,
  required final String sha256,
  required final int sizeBytes,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceTextSaveResult(
    relativePath: json.requiredString('relativePath'),
    sha256: json.requiredString('sha256'),
    sizeBytes: json['sizeBytes'] is int ? json['sizeBytes']! as int : 0,
  );
}

class const MobileWorkspaceTrashEntry({
  required final String id,
  required final String relativePath,
  required final String sha256,
  required final int sizeBytes,
  required final int deletedAtMs,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceTrashEntry(
    id: json.requiredString('id'),
    relativePath: json.requiredString('relativePath'),
    sha256: json.requiredString('sha256'),
    sizeBytes: json['sizeBytes'] is int ? json['sizeBytes']! as int : 0,
    deletedAtMs: json['deletedAtMs'] is int ? json['deletedAtMs']! as int : 0,
  );
}

class const MobileWorkspaceTrashList({
  required final List<MobileWorkspaceTrashEntry> items,
  required final int limit,
  required final int capacityBytes,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceTrashList(
    items: <MobileWorkspaceTrashEntry>[
      for (final item in json['items'] as List<Object?>? ?? const <Object?>[])
        if (item is Map) MobileWorkspaceTrashEntry.fromJson(asJsonMap(item)),
    ],
    limit: json['limit'] is int ? json['limit']! as int : 64,
    capacityBytes: json['capacityBytes'] is int
        ? json['capacityBytes']! as int
        : 16 * 1024 * 1024,
  );
}

class const MobileWorkspaceDirectoryEntry({
  required final String name,
  required final String relativePath,
  required final bool isDirectory,
  required final int sizeBytes,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceDirectoryEntry(
    name: json.requiredString('name'),
    relativePath: json.requiredString('relativePath'),
    isDirectory: json['isDirectory'] == true,
    sizeBytes: json['sizeBytes'] is int ? json['sizeBytes']! as int : 0,
  );
}

class const MobileWorkspaceDirectoryPage({
  required final List<MobileWorkspaceDirectoryEntry> items,
  required final int nextOffset,
  required final bool hasMore,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceDirectoryPage(
    items: <MobileWorkspaceDirectoryEntry>[
      for (final item in json['items'] as List<Object?>? ?? const <Object?>[])
        if (item is Map)
          MobileWorkspaceDirectoryEntry.fromJson(asJsonMap(item)),
    ],
    nextOffset: json['nextOffset'] is int ? json['nextOffset']! as int : 0,
    hasMore: json['hasMore'] == true,
  );
}

const int maxMobileWorkspacePreviewBytes = 32 * 1024 * 1024;
const int maxMobileWorkspaceShareBytes = 256 * 1024 * 1024;
const int maxPromptFileBytes = 32 * 1024 * 1024;
const int maxPromptFileChunkBytes = 256 * 1024;

class const MobileWorkspaceQuickOpenSession({
  required final String id,
  required final int indexedFileCount,
}) {
  factory fromJson(Map<String, Object?> json) =>
      MobileWorkspaceQuickOpenSession(
        id: json.requiredString('sessionId'),
        indexedFileCount: json['indexedFileCount'] is int
            ? json['indexedFileCount']! as int
            : 0,
      );
}

class const MobileWorkspaceQuickOpenMatch({
  required final String relativePath,
  required final int score,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceQuickOpenMatch(
    relativePath: json.requiredString('relativePath'),
    score: json['score'] is int ? json['score']! as int : 0,
  );
}

class const MobileCodexSavedPrompt({
  required final String name,
  required final String description,
  required final String body,
  required final String scope,
  final String? argumentHint,
}) {
  factory fromJson(Map<String, Object?> json) => MobileCodexSavedPrompt(
    name: json.requiredString('name'),
    description: json.requiredString('description'),
    body: json['body'] is String
        ? json['body']! as String
        : throw const FormatException('body is required'),
    scope: json.requiredString('scope'),
    argumentHint: json.optionalString('argumentHint'),
  );
}

class const MobileWorkspaceFileRange({
  required final String relativePath,
  required final int offset,
  required final int nextOffset,
  required final int totalBytes,
  required final String mimeType,
  required final bool isText,
  required final List<int> bytes,
});

class const MobileWorkspaceFileDiff({
  required final String relativePath,
  required final String patch,
  required final bool truncated,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceFileDiff(
    relativePath: json.requiredString('relativePath'),
    patch: json['patch'] as String? ?? '',
    truncated: json['truncated'] == true,
  );
}

class const MobileWorkspaceGitStatusEntry({
  required final String relativePath,
  required final String status,
  final bool staged = false,
  final bool unstaged = false,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitStatusEntry(
    relativePath: json.requiredString('relativePath'),
    status: json['status'] as String? ?? 'modified',
    staged: json['staged'] == true,
    unstaged: json['unstaged'] == true,
  );
}

class const MobileWorkspaceGitCommit({
  required final String id,
  required final String summary,
  required final String message,
  required final String author,
  required final int timestamp,
  required final List<String> parents,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitCommit(
    id: json.requiredString('id'),
    summary: json['summary'] as String? ?? '',
    message: json['message'] as String? ?? '',
    author: json['author'] as String? ?? '',
    timestamp: json['timestamp'] is int ? json['timestamp']! as int : 0,
    parents: <String>[
      for (final parent
          in json['parents'] as List<Object?>? ?? const <Object?>[])
        if (parent is String) parent,
    ],
  );
}

class const MobileWorkspaceGitHistory({
  required final String branch,
  required final List<MobileWorkspaceGitCommit> commits,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitHistory(
    branch: json['branch'] as String? ?? 'HEAD',
    commits: <MobileWorkspaceGitCommit>[
      for (final item in json['commits'] as List<Object?>? ?? const <Object?>[])
        if (item is Map) MobileWorkspaceGitCommit.fromJson(asJsonMap(item)),
    ],
  );
}

class const MobileWorkspaceGitChangedFile({
  required final String relativePath,
  required final String status,
  final String? oldPath,
  final String? newPath,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitChangedFile(
    relativePath: json.requiredString('relativePath'),
    status: json['status'] as String? ?? 'modified',
    oldPath: json.optionalString('oldPath'),
    newPath: json.optionalString('newPath'),
  );
}

class const MobileWorkspaceGitComparison({
  required final String? baseCommit,
  required final String targetCommit,
  required final List<MobileWorkspaceGitChangedFile> items,
  required final bool truncated,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitComparison(
    baseCommit: json.optionalString('baseCommit'),
    targetCommit: json.requiredString('targetCommit'),
    items: <MobileWorkspaceGitChangedFile>[
      for (final item in json['items'] as List<Object?>? ?? const <Object?>[])
        if (item is Map)
          MobileWorkspaceGitChangedFile.fromJson(asJsonMap(item)),
    ],
    truncated: json['truncated'] == true,
  );
}

class const MobileWorkspaceGitRevisionPatch({
  required final String? baseCommit,
  required final String targetCommit,
  required final String relativePath,
  required final String patch,
  required final bool truncated,
}) {
  factory fromJson(Map<String, Object?> json) =>
      MobileWorkspaceGitRevisionPatch(
        baseCommit: json.optionalString('baseCommit'),
        targetCommit: json.requiredString('targetCommit'),
        relativePath: json.requiredString('relativePath'),
        patch: json['patch'] as String? ?? '',
        truncated: json['truncated'] == true,
      );
}

class const MobileWorkspaceGitStatus({
  required final List<MobileWorkspaceGitStatusEntry> items,
  required final bool truncated,
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitStatus(
    items: <MobileWorkspaceGitStatusEntry>[
      for (final item in json['items'] as List<Object?>? ?? const <Object?>[])
        if (item is Map)
          MobileWorkspaceGitStatusEntry.fromJson(asJsonMap(item)),
    ],
    truncated: json['truncated'] == true,
  );
}

bool mobileWorkspaceFileCanShare(int totalBytes) =>
    totalBytes >= 0 && totalBytes <= maxMobileWorkspaceShareBytes;

class const PromptFileUploadResult({required final String hostPath}) {
  factory fromJson(Map<String, Object?> json) =>
      PromptFileUploadResult(hostPath: json.requiredString('path'));
}
