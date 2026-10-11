import 'dart:math' as math;
import 'dart:typed_data';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';

/// Image data is decoded only by Flutter's local decoder, never by a WebView
/// and never loaded from a remote URL embedded in workspace content.
const int maxMobileWorkspaceImagePreviewBytes = 4 * 1024 * 1024;

bool isSupportedMobileWorkspaceImageMime(String mime) => const <String>{
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
}.contains(mime);

class MobileWorkspaceImagePreview {
  const MobileWorkspaceImagePreview({
    required this.relativePath,
    required this.mimeType,
    required this.bytes,
  });

  final String relativePath;
  final String mimeType;
  final Uint8List bytes;
}

/// Read at most 4 MiB, checking consistent path/size/MIME/byte offsets across
/// chunks and a minimal image signature before passing bytes to the decoder.
Future<MobileWorkspaceImagePreview> loadWorkspaceImagePreview(
  Future<MobileWorkspaceFileRange> Function(int offset, int length) read, {
  int maxBytes = maxMobileWorkspaceImagePreviewBytes,
}) async {
  if (maxBytes <= 0 || maxBytes > maxMobileWorkspaceImagePreviewBytes) {
    throw ArgumentError.value(maxBytes, 'maxBytes');
  }
  final data = BytesBuilder(copy: false);
  String? path;
  String? mime;
  int? total;
  var offset = 0;
  do {
    final requested = math.min(
      maxMobileWorkspaceFileRangeBytes,
      maxBytes - offset,
    );
    final page = await read(offset, requested);
    if (page.offset != offset ||
        page.nextOffset != offset + page.bytes.length ||
        page.bytes.isEmpty ||
        page.totalBytes <= 0 ||
        page.totalBytes > maxBytes ||
        page.nextOffset > page.totalBytes ||
        page.bytes.length > requested ||
        page.bytes.length > maxMobileWorkspaceFileRangeBytes ||
        page.isText ||
        !isSupportedMobileWorkspaceImageMime(page.mimeType) ||
        (path != null && path != page.relativePath) ||
        (mime != null && mime != page.mimeType) ||
        (total != null && total != page.totalBytes)) {
      throw const FormatException(
        'Invalid or changed workspace image preview.',
      );
    }
    path = page.relativePath;
    mime = page.mimeType;
    total = page.totalBytes;
    data.add(page.bytes);
    offset = page.nextOffset;
  } while (offset < total);
  final bytes = data.takeBytes();
  if (!_hasImageMagic(bytes, mime)) {
    throw const FormatException('Image contents do not match its file type.');
  }
  return MobileWorkspaceImagePreview(
    relativePath: path,
    mimeType: mime,
    bytes: bytes,
  );
}

bool _hasImageMagic(Uint8List bytes, String mime) {
  bool startsWith(List<int> magic) =>
      bytes.length >= magic.length &&
      List<bool>.generate(
        magic.length,
        (i) => bytes[i] == magic[i],
      ).every((matches) => matches);
  return switch (mime) {
    'image/png' => startsWith(<int>[137, 80, 78, 71, 13, 10, 26, 10]),
    'image/jpeg' => startsWith(<int>[255, 216, 255]),
    'image/gif' =>
      startsWith(<int>[71, 73, 70, 56, 55, 97]) ||
          startsWith(<int>[71, 73, 70, 56, 57, 97]),
    'image/webp' =>
      bytes.length >= 12 &&
          startsWith(<int>[82, 73, 70, 70]) &&
          bytes[8] == 87 &&
          bytes[9] == 69 &&
          bytes[10] == 66 &&
          bytes[11] == 80,
    _ => false,
  };
}
