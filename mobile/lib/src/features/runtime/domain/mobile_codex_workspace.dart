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
}) {
  factory fromJson(Map<String, Object?> json) => MobileWorkspaceGitStatusEntry(
    relativePath: json.requiredString('relativePath'),
    status: json['status'] as String? ?? 'modified',
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
