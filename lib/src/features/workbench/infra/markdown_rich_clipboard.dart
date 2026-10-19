import 'dart:typed_data';

import 'package:flutter/services.dart';
import 'package:markdown/markdown.dart' as md;

/// The HTML and plain-text representations of one copied Markdown document.
/// Rich paste targets use HTML; code editors can still paste readable text.
class const MarkdownRichClipboardData({
  required this.html,
  required this.plainText,
}) {
  final String html;
  final String plainText;
}

class MarkdownRichClipboard {
  const MarkdownRichClipboard();

  static const MethodChannel channel = MethodChannel(
    'dev.leynier.alera/rich_clipboard',
  );

  /// Use a native multi-format clipboard instead of Flutter's text-only API.
  /// An unimplemented platform falls back to plain text without claiming it
  /// copied formatting.
  Future<bool> copy(MarkdownRichClipboardData data) async {
    try {
      await channel.invokeMethod<void>('setHtml', <String, String>{
        'html': data.html,
        'text': data.plainText,
      });
      return true;
    } on MissingPluginException {
      await Clipboard.setData(ClipboardData(text: data.plainText));
      return false;
    }
  }

  /// A separate image/diagram copy uses native image clipboard MIME, which
  /// email composers recognize more consistently than an HTML image alone.
  Future<void> copyPng(Uint8List png) =>
      channel.invokeMethod<void>('setImagePng', png);
}

MarkdownRichClipboardData markdownRichClipboardDocument(String markdown) {
  final markup = _styleMarkdownHtml(markdownClipboardRawHtml(markdown));
  return MarkdownRichClipboardData(
    html: markup,
    plainText: _htmlToPlainText(markup),
  );
}

/// Internal Markdown rendering shared with the async image/diagram export.
String markdownClipboardRawHtml(String markdown) => md.markdownToHtml(
  _escapeRawHtml(markdown),
  extensionSet: md.ExtensionSet.gitHubFlavored,
  encodeHtml: true,
);

/// Apply the same Office styles, sanitization and plain-text fallback to
/// markup after its media references have been replaced by embedded images.
MarkdownRichClipboardData markdownClipboardFromHtml(String rawHtml) {
  final html = _styleMarkdownHtml(rawHtml);
  return MarkdownRichClipboardData(
    html: html,
    plainText: _htmlToPlainText(html),
  );
}

/// Keeps table structure, including true TH/TD cells, rather than copying the
/// visual text of independent Flutter table cells as tab-separated strings.
MarkdownRichClipboardData markdownRichClipboardTable(
  List<MarkdownRichTableRow> rows,
) {
  return markdownClipboardFromHtml(markdownClipboardRawTableHtml(rows));
}

String markdownClipboardRawTableHtml(List<MarkdownRichTableRow> rows) {
  final markup = StringBuffer('<table>');
  for (final row in rows) {
    markup.write('<tr>');
    for (final cell in row.cells) {
      final tag = row.isHeader ? 'th' : 'td';
      markup
        ..write('<$tag>')
        ..write(
          md.markdownToHtml(
            _escapeRawHtml(cell),
            extensionSet: md.ExtensionSet.gitHubFlavored,
            encodeHtml: true,
            inlineOnly: true,
          ),
        )
        ..write('</$tag>');
    }
    markup.write('</tr>');
  }
  markup.write('</table>');
  return markup.toString();
}

class const MarkdownRichTableRow({
  required this.cells,
  required this.isHeader,
}) {
  final List<String> cells;
  final bool isHeader;
}

String _escapeRawHtml(String source) {
  // `encodeHtml` escapes Markdown text, but the markdown parser deliberately
  // passes raw HTML tags through. Rich clipboard content should not inject
  // arbitrary HTML into an email composer or other paste destination. Keep
  // code fences and inline code untouched: their angle brackets are literal.
  final lines = source.split('\n');
  String? fence;
  var fenceLength = 0;
  final result = <String>[];
  final fencePattern = RegExp(r'^ {0,3}(`{3,}|~{3,})(.*)$');
  for (final line in lines) {
    final marker = fencePattern.firstMatch(line);
    if (fence != null) {
      result.add(line);
      if (marker != null &&
          marker.group(1)![0] == fence &&
          marker.group(1)!.length >= fenceLength &&
          marker.group(2)!.trim().isEmpty) {
        fence = null;
      }
      continue;
    }
    if (marker != null) {
      fence = marker.group(1)![0];
      fenceLength = marker.group(1)!.length;
      result.add(line);
      continue;
    }
    final buffer = StringBuffer();
    var start = 0;
    for (final code in RegExp(r'(`+)(.*?)\1').allMatches(line)) {
      buffer
        ..write(_escapeHtmlTags(line.substring(start, code.start)))
        ..write(code.group(0));
      start = code.end;
    }
    buffer.write(_escapeHtmlTags(line.substring(start)));
    result.add(buffer.toString());
  }
  return result.join('\n');
}

String _escapeHtmlTags(String source) {
  return source.replaceAllMapped(RegExp(r'</?[a-zA-Z][^>\r\n]*>'), (match) {
    final tag = match.group(0)!;
    // A bare HTML <br> is a safe, semantic Markdown line break. Leave any
    // attributes or other HTML escaped, so a rich paste cannot inject markup.
    if (RegExp(r'^<br\s*/?>$', caseSensitive: false).hasMatch(tag)) {
      return '<br />';
    }
    if (tag.startsWith('<https://') ||
        tag.startsWith('<http://') ||
        tag.startsWith('<mailto:')) {
      return tag; // Standard Markdown auto-links.
    }
    return '&lt;${tag.substring(1, tag.length - 1)}&gt;';
  });
}

String _styleMarkdownHtml(String html) {
  // Markdown generates semantic tags; inline styles survive HTML clipboard
  // consumers such as Word and Outlook without relying on external CSS.
  const styles = <String, String>{
    'h1': 'font-size:24pt;font-weight:700;margin:14pt 0 8pt',
    'h2': 'font-size:18pt;font-weight:700;margin:12pt 0 6pt',
    'h3': 'font-size:14pt;font-weight:700;margin:10pt 0 5pt',
    'h4': 'font-size:12pt;font-weight:700;margin:8pt 0 4pt',
    'p': 'margin:0 0 8pt;line-height:1.45',
    'table': 'border-collapse:collapse;border-spacing:0;margin:8pt 0',
    'th': 'border:1px solid #aeb4bf;padding:5px 9px;font-weight:700;background:#eceff3',
    'td': 'border:1px solid #aeb4bf;padding:5px 9px',
    'blockquote': 'border-left:3px solid #aeb4bf;padding-left:12px;margin-left:6px;color:#555',
    'pre': 'font-family:Consolas,monospace;white-space:pre-wrap;background:#f2f3f5;padding:10px',
    'code': 'font-family:Consolas,monospace',
    'ul': 'margin:5pt 0;padding-left:22pt',
    'ol': 'margin:5pt 0;padding-left:22pt',
  };
  var result = html;
  for (final entry in styles.entries) {
    result = result.replaceAll(
      RegExp('<${entry.key}(?=[\\s>])', caseSensitive: false),
      '<${entry.key} style="${entry.value}"',
    );
  }

  // Never pass workspace-relative paths, file URIs, or executable schemes to
  // external paste consumers. Keep safe web/mail links and remote images.
  result = result.replaceAllMapped(
    RegExp(r'\b(href|src)="([^"]*)"', caseSensitive: false),
    (match) {
      final attribute = match.group(1)!.toLowerCase();
      final raw = match.group(2)!;
      final uri = Uri.tryParse(raw.replaceAll('&amp;', '&'));
      final embeddedImage =
          attribute == 'src' &&
          RegExp(r'^data:image/(?:png|jpeg);base64,[a-zA-Z0-9+/=]+$')
              .hasMatch(raw) &&
          raw.length <= 24 * 1024 * 1024;
      final permitted =
          embeddedImage ||
          uri != null &&
              ((uri.scheme == 'http' || uri.scheme == 'https') &&
                      uri.hasAuthority ||
                  (attribute == 'href' && uri.scheme == 'mailto'));
      return permitted ? match.group(0)! : '$attribute=""';
    },
  );
  return result;
}

String _htmlToPlainText(String html) {
  var text = html
      .replaceAllMapped(
        RegExp(r'<img\b[^>]*alt="([^"]*)"[^>]*>', caseSensitive: false),
        (match) => '[Image: ${match.group(1)}]',
      )
      .replaceAll(RegExp(r'</(?:th|td)>', caseSensitive: false), '\t')
      .replaceAll(
        RegExp(
          r'</(?:tr|p|h[1-6]|li|pre|blockquote|div)>',
          caseSensitive: false,
        ),
        '\n',
      )
      .replaceAll(
        // Markdown serializes a hard break as <br /> followed by a source
        // newline. Treat the pair as one break in the plain-text fallback.
        RegExp(r'<br\s*/?>[ \t]*(?:\r?\n)?', caseSensitive: false),
        '\n',
      )
      .replaceAll(RegExp(r'<li\b[^>]*>', caseSensitive: false), '• ')
      .replaceAll(RegExp(r'<[^>]+>'), '');
  text = text.replaceAllMapped(
    RegExp(r'&(?:#[xX][0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);'),
    (match) {
      final entity = match.group(0)!;
      if (entity.startsWith('&#')) {
        final hexadecimal = entity[2].toLowerCase() == 'x';
        final value = int.tryParse(
          entity.substring(hexadecimal ? 3 : 2, entity.length - 1),
          radix: hexadecimal ? 16 : 10,
        );
        return value != null && value >= 0 && value <= 0x10ffff
            ? String.fromCharCode(value)
            : entity;
      }
      return switch (entity) {
        '&amp;' => '&',
        '&lt;' => '<',
        '&gt;' => '>',
        '&quot;' => '"',
        '&apos;' || '&#39;' => "'",
        '&nbsp;' => ' ',
        _ => entity,
      };
    },
  );
  return text
      .split('\n')
      .map((line) => line.trimRight())
      .join('\n')
      .replaceAll(RegExp(r'\n{3,}'), '\n\n')
      .trim();
}
