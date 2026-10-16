import 'dart:convert';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/infra/markdown_rich_clipboard.dart';

/// An already validated, portable PNG or JPEG image, never a file path.
class MarkdownClipboardImage {
  const MarkdownClipboardImage({required this.bytes, required this.mimeType});

  final Uint8List bytes;
  final String mimeType;
}

typedef MarkdownClipboardImageResolver =
    Future<MarkdownClipboardImage?> Function(String source);
typedef MarkdownClipboardDiagramRenderer =
    Future<MarkdownClipboardImage?> Function(String source, String diagramId);

// Limits keep a document with many large images from filling the clipboard or
// freezing Word/Outlook. Oversized and unavailable media retains an alt label.
const int markdownClipboardMaxImages = 24;
const int markdownClipboardMaxImageBytes = 4 * 1024 * 1024;
const int markdownClipboardMaxTotalMediaBytes = 12 * 1024 * 1024;

Future<MarkdownRichClipboardData> markdownRichClipboardDocumentWithMedia({
  required String markdown,
  required MarkdownClipboardImageResolver imageResolver,
  MarkdownClipboardDiagramRenderer? diagramRenderer,
  String diagramIdPrefix = 'markdown-clipboard',
}) async {
  var html = markdownClipboardRawHtml(markdown);
  final budget = _MediaBudget();
  html = await _replaceMarkdownImages(html, imageResolver, budget);
  if (diagramRenderer != null) {
    html = await _replaceMermaidFences(
      html,
      diagramRenderer,
      diagramIdPrefix,
      budget,
    );
  }
  return markdownClipboardFromHtml(html);
}

Future<MarkdownRichClipboardData> markdownRichClipboardTableWithMedia({
  required List<MarkdownRichTableRow> rows,
  required MarkdownClipboardImageResolver imageResolver,
}) async {
  final html = await _replaceMarkdownImages(
    markdownClipboardRawTableHtml(rows),
    imageResolver,
    _MediaBudget(),
  );
  return markdownClipboardFromHtml(html);
}

class _MediaBudget {
  int count = 0;
  int bytes = 0;

  bool include(MarkdownClipboardImage image) {
    if (image.mimeType != 'image/png' && image.mimeType != 'image/jpeg') {
      return false;
    }
    if (image.bytes.isEmpty ||
        image.bytes.length > markdownClipboardMaxImageBytes ||
        count >= markdownClipboardMaxImages ||
        bytes + image.bytes.length > markdownClipboardMaxTotalMediaBytes) {
      return false;
    }
    count++;
    bytes += image.bytes.length;
    return true;
  }
}

Future<String> _replaceMarkdownImages(
  String html,
  MarkdownClipboardImageResolver resolver,
  _MediaBudget budget,
) async {
  final tags = RegExp(r'<img\b[^>]*>', caseSensitive: false).allMatches(html);
  final out = StringBuffer();
  var end = 0;
  final cache = <String, MarkdownClipboardImage?>{};
  for (final tag in tags) {
    out.write(html.substring(end, tag.start));
    end = tag.end;
    final raw = tag.group(0)!;
    final src = RegExp(r'\bsrc="([^"]*)"').firstMatch(raw)?.group(1);
    final alt = RegExp(r'\balt="([^"]*)"').firstMatch(raw)?.group(1) ?? 'Image';
    if (src == null || src.isEmpty) {
      out.write(_missingMedia(alt));
      continue;
    }
    final source = _decodeHtmlEntities(src);
    MarkdownClipboardImage? image;
    if (cache.containsKey(source)) {
      image = cache[source];
    } else if (budget.count < markdownClipboardMaxImages) {
      try {
        image = await resolver(source);
      } catch (_) {
        image = null;
      }
      cache[source] = image;
    }
    if (image == null || !budget.include(image)) {
      out.write(_missingMedia(alt));
      continue;
    }
    final dataUrl =
        'data:${image.mimeType};base64,${base64Encode(image.bytes)}';
    out.write(raw.replaceFirst('src="$src"', 'src="$dataUrl"'));
  }
  out.write(html.substring(end));
  return out.toString();
}

Future<String> _replaceMermaidFences(
  String html,
  MarkdownClipboardDiagramRenderer renderer,
  String diagramIdPrefix,
  _MediaBudget budget,
) async {
  final blocks = RegExp(
    r'<pre(?:\s[^>]*)?><code\s+class="language-(?:mermaid|mmd|merman)"[^>]*>([\s\S]*?)</code></pre>',
    caseSensitive: false,
  ).allMatches(html);
  final out = StringBuffer();
  var end = 0;
  var number = 0;
  for (final block in blocks) {
    out.write(html.substring(end, block.start));
    end = block.end;
    final source = _decodeHtmlEntities(block.group(1)!).trimRight();
    MarkdownClipboardImage? png;
    if (budget.count < markdownClipboardMaxImages) {
      try {
        png = await renderer(source, '$diagramIdPrefix:${number++}');
      } catch (_) {
        png = null;
      }
    }
    if (png == null || !budget.include(png)) {
      // Rendering failure must never discard the original diagram source.
      out.write(block.group(0));
      continue;
    }
    out.write(
      '<img alt="Mermaid diagram" '
      'src="data:${png.mimeType};base64,${base64Encode(png.bytes)}" />',
    );
  }
  out.write(html.substring(end));
  return out.toString();
}

String _missingMedia(String alt) => '<span>[Image: $alt]</span>';

String _decodeHtmlEntities(String text) => text
    .replaceAll('&quot;', '"')
    .replaceAll('&#39;', "'")
    .replaceAll('&lt;', '<')
    .replaceAll('&gt;', '>')
    .replaceAll('&amp;', '&');
