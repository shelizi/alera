import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';

const int maxMobileWorkspaceTextPreviewBytes = 512 * 1024;

class const MobileWorkspaceTextPreview({
  required final String relativePath,
  required final String text,
  required final bool isText,
  required final int totalBytes,
  required final bool truncated,
  required final String mimeType,
});

/// Assemble remote byte ranges before decoding. UTF-8 characters can cross
/// WebSocket frame boundaries; never try to render binary files as text.
Future<MobileWorkspaceTextPreview> loadWorkspaceTextPreview(
  Future<MobileWorkspaceFileRange> Function(int offset, int length) read, {
  int maxBytes = maxMobileWorkspaceTextPreviewBytes,
}) async {
  if (maxBytes <= 0) throw ArgumentError.value(maxBytes, 'maxBytes');
  final bytes = BytesBuilder(copy: false);
  var offset = 0;
  int? expectedTotalBytes;
  String? expectedPath;
  var mimeType = '';
  while (offset < maxBytes) {
    final requested = math.min(
      maxMobileWorkspaceFileRangeBytes,
      maxBytes - offset,
    );
    final page = await read(offset, requested);
    if (page.offset != offset ||
        page.nextOffset != offset + page.bytes.length ||
        page.bytes.length > requested ||
        page.totalBytes < 0 ||
        page.nextOffset > page.totalBytes ||
        (expectedTotalBytes != null && expectedTotalBytes != page.totalBytes) ||
        (expectedPath != null && expectedPath != page.relativePath)) {
      throw const FormatException('Workspace file changed during preview.');
    }
    expectedTotalBytes = page.totalBytes;
    expectedPath = page.relativePath;
    mimeType = page.mimeType;
    if (!page.isText) {
      return MobileWorkspaceTextPreview(
        relativePath: page.relativePath,
        text: '',
        isText: false,
        totalBytes: page.totalBytes,
        truncated: false,
        mimeType: mimeType,
      );
    }
    bytes.add(page.bytes);
    offset = page.nextOffset;
    if (offset == expectedTotalBytes) break;
    if (page.bytes.isEmpty) {
      throw const FormatException('Workspace file preview made no progress.');
    }
  }
  return MobileWorkspaceTextPreview(
    relativePath: expectedPath ?? '',
    text: utf8.decode(bytes.takeBytes(), allowMalformed: true),
    isText: true,
    totalBytes: expectedTotalBytes ?? 0,
    truncated: offset < (expectedTotalBytes ?? 0),
    mimeType: mimeType,
  );
}
