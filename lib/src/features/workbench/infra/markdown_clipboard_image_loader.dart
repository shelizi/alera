import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:alera/src/features/workbench/infra/markdown_rich_clipboard_media.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_uri_policy.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_viewer_images.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:image/image.dart' as img;

const Duration markdownClipboardRemoteTimeout = Duration(seconds: 9);
const int markdownClipboardMaxSourceBytes = 4 * 1024 * 1024;

/// Loads the exact Markdown Preview image source, preventing workspace escape
/// and copying bytes rather than file:// references that break in mail/Word.
class MarkdownClipboardImageLoader {
  const MarkdownClipboardImageLoader({
    required this.workspacePath,
    required this.markdownPath,
  });

  final String workspacePath;
  final String markdownPath;

  Future<MarkdownClipboardImage?> load(String source) async {
    final uri = Uri.tryParse(source);
    if (isSupportedMarkdownViewerRemoteImageUri(uri)) {
      if (!_isSafeRemoteImageUri(uri!)) return null;
      final bytes = await _downloadImage(uri);
      return bytes == null ? null : markdownClipboardImageFromBytes(bytes);
    }

    final filePath = await resolveWorkspaceMarkdownImageFilePath(
      workspacePath: workspacePath,
      markdownPath: markdownPath,
      rawImageUrl: source,
    );
    if (filePath == null) return null;
    final file = File(filePath);
    final size = await file.length();
    if (size == 0 || size > markdownClipboardMaxSourceBytes) return null;
    return markdownClipboardImageFromBytes(await file.readAsBytes());
  }
}

bool _isSafeRemoteImageUri(Uri uri) {
  if (uri.userInfo.isNotEmpty || uri.host.isEmpty) return false;
  final host = uri.host.toLowerCase();
  if (host == 'localhost' ||
      host.endsWith('.localhost') ||
      host.endsWith('.local'))
    return false;
  final ip = InternetAddress.tryParse(host);
  if (ip != null && (ip.isLoopback || ip.isLinkLocal || _isPrivateIp(ip))) {
    return false;
  }
  return true;
}

bool _isPrivateIp(InternetAddress ip) {
  final bytes = ip.rawAddress;
  if (ip.type == InternetAddressType.IPv4) {
    return bytes[0] == 10 ||
        bytes[0] == 127 ||
        bytes[0] == 0 ||
        (bytes[0] == 172 && bytes[1] >= 16 && bytes[1] <= 31) ||
        (bytes[0] == 192 && bytes[1] == 168) ||
        (bytes[0] == 169 && bytes[1] == 254);
  }
  return bytes.isNotEmpty &&
      ((bytes[0] & 0xfe) == 0xfc ||
          (bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80));
}

Future<Uint8List?> _downloadImage(Uri url) async {
  final client = HttpClient()
    ..connectionTimeout = markdownClipboardRemoteTimeout
    ..idleTimeout = const Duration(seconds: 3);
  try {
    final request = await client
        .getUrl(url)
        .timeout(markdownClipboardRemoteTimeout);
    request.followRedirects = false;
    final response = await request.close().timeout(
      markdownClipboardRemoteTimeout,
    );
    if (response.statusCode != HttpStatus.ok ||
        response.contentLength > markdownClipboardMaxSourceBytes) {
      return null;
    }
    final chunks = BytesBuilder(copy: false);
    await for (final chunk in response.timeout(
      markdownClipboardRemoteTimeout,
    )) {
      if (chunks.length + chunk.length > markdownClipboardMaxSourceBytes) {
        return null;
      }
      chunks.add(chunk);
    }
    return chunks.takeBytes();
  } catch (_) {
    return null;
  } finally {
    client.close(force: true);
  }
}

/// Converts GIF/WebP and SVG images to portable PNG for Word/Outlook, while
/// preserving normal PNG/JPEG bytes (no lossy recompression).
Future<MarkdownClipboardImage?> markdownClipboardImageFromBytes(
  Uint8List bytes,
) async {
  if (bytes.isEmpty || bytes.length > markdownClipboardMaxSourceBytes) {
    return null;
  }
  if (_isPng(bytes)) {
    return MarkdownClipboardImage(bytes: bytes, mimeType: 'image/png');
  }
  if (bytes.length > 2 && bytes[0] == 0xff && bytes[1] == 0xd8) {
    return MarkdownClipboardImage(bytes: bytes, mimeType: 'image/jpeg');
  }
  if (_isSvg(bytes)) {
    try {
      return await markdownClipboardRasterizeSvg(utf8.decode(bytes));
    } catch (_) {
      return null;
    }
  }
  final png = await compute(_decodeRasterAsPng, bytes);
  return png == null || png.length > markdownClipboardMaxImageBytes
      ? null
      : MarkdownClipboardImage(bytes: png, mimeType: 'image/png');
}

Future<Uint8List?> markdownClipboardPngBytes(MarkdownClipboardImage image) {
  if (image.mimeType == 'image/png') return Future.value(image.bytes);
  if (image.mimeType != 'image/jpeg') return Future.value(null);
  return compute(_decodeRasterAsPng, image.bytes);
}

bool _isPng(Uint8List bytes) =>
    bytes.length >= 8 &&
    bytes[0] == 137 &&
    bytes[1] == 80 &&
    bytes[2] == 78 &&
    bytes[3] == 71 &&
    bytes[4] == 13 &&
    bytes[5] == 10 &&
    bytes[6] == 26 &&
    bytes[7] == 10;

bool _isSvg(Uint8List bytes) {
  final header = String.fromCharCodes(bytes.take(math.min(bytes.length, 256)))
      .trimLeft();
  return header.startsWith('<svg') ||
      (header.startsWith('<?xml') && header.contains('<svg'));
}

Uint8List? _decodeRasterAsPng(Uint8List bytes) {
  final original = img.decodeImage(bytes);
  if (original == null ||
      original.width <= 0 ||
      original.height <= 0 ||
      original.width * original.height > 16 * 1024 * 1024)
    return null;
  final longSide = math.max(original.width, original.height);
  final raster = longSide > 1600
      ? img.copyResize(
          original,
          width: original.width >= original.height ? 1600 : null,
          height: original.height > original.width ? 1600 : null,
        )
      : original;
  return Uint8List.fromList(img.encodePng(raster));
}

/// Rasterizes the same SVG produced by the Mermaid Preview into a white-backed
/// PNG, so it remains legible when pasted into light Word/email documents.
Future<MarkdownClipboardImage?> markdownClipboardRasterizeSvg(
  String svg,
) async {
  if (svg.length > markdownClipboardMaxSourceBytes ||
      RegExp(
        r'<(?:script|foreignObject)\b',
        caseSensitive: false,
      ).hasMatch(svg))
    return null;
  final info = await vg.loadPicture(SvgStringLoader(svg), null);
  try {
    final size = info.size;
    if (!size.width.isFinite ||
        !size.height.isFinite ||
        size.width <= 0 ||
        size.height <= 0)
      return null;
    final scale = math.min(2.0, 1600 / math.max(size.width, size.height));
    final width = math.max(1, (size.width * scale).ceil());
    final height = math.max(1, (size.height * scale).ceil());
    if (width * height > 4 * 1024 * 1024) return null;
    final recorder = ui.PictureRecorder();
    final canvas = ui.Canvas(recorder)
      ..drawColor(const ui.Color(0xffffffff), ui.BlendMode.src)
      ..scale(width / size.width, height / size.height)
      ..drawPicture(info.picture);
    final picture = recorder.endRecording();
    try {
      final image = await picture.toImage(width, height);
      try {
        final data = await image.toByteData(format: ui.ImageByteFormat.png);
        if (data == null ||
            data.lengthInBytes > markdownClipboardMaxImageBytes) {
          return null;
        }
        return MarkdownClipboardImage(
          bytes: data.buffer.asUint8List(),
          mimeType: 'image/png',
        );
      } finally {
        image.dispose();
      }
    } finally {
      picture.dispose();
    }
  } finally {
    info.picture.dispose();
  }
}
