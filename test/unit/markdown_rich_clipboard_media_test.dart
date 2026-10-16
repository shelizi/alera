import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/infra/markdown_clipboard_image_loader.dart';
import 'package:alera/src/features/workbench/infra/markdown_rich_clipboard.dart';
import 'package:alera/src/features/workbench/infra/markdown_rich_clipboard_media.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/services.dart';
import 'package:image/image.dart' as img;

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final imageBytes = Uint8List.fromList(
    img.encodePng(img.Image(width: 2, height: 2)),
  );
  final image = MarkdownClipboardImage(
    bytes: imageBytes,
    mimeType: 'image/png',
  );

  test(
    'embeds local images and Mermaid in the original document order',
    () async {
      final sources = <String>[];
      final diagrams = <String>[];
      final copied = await markdownRichClipboardDocumentWithMedia(
        markdown: '''# Report

Before.

![Chart](charts/revenue.png)

Between.

```mermaid
flowchart TD
  A --> B
```

After.
''',
        imageResolver: (source) async {
          sources.add(source);
          return image;
        },
        diagramRenderer: (source, diagramId) async {
          diagrams.add(source);
          expect(diagramId, startsWith('docs/report.md:'));
          return image;
        },
        diagramIdPrefix: 'docs/report.md',
      );

      final dataUrl = 'data:image/png;base64,${base64Encode(imageBytes)}';
      expect(sources, ['charts/revenue.png']);
      expect(diagrams.single, contains('A --> B'));
      expect(
        RegExp('src="data:image/png;base64,').allMatches(copied.html),
        hasLength(2),
      );
      expect(copied.html, contains(dataUrl));
      expect(copied.html, isNot(contains('src="charts/revenue.png"')));
      expect(copied.html, isNot(contains('language-mermaid')));
      expect(
        copied.html.indexOf('Before.'),
        lessThan(copied.html.indexOf('Chart')),
      );
      expect(
        copied.html.indexOf('Chart'),
        lessThan(copied.html.indexOf('Between.')),
      );
      expect(
        copied.html.indexOf('Between.'),
        lessThan(copied.html.indexOf('Mermaid diagram')),
      );
      expect(
        copied.html.indexOf('Mermaid diagram'),
        lessThan(copied.html.indexOf('After.')),
      );
      expect(copied.plainText, contains('[Image: Chart]'));
      expect(copied.plainText, contains('[Image: Mermaid diagram]'));
    },
  );

  test(
    'unavailable images show alt text and failed diagram keeps its source',
    () async {
      final result = await markdownRichClipboardDocumentWithMedia(
        markdown: '''![Missing](missing.jpg)

```mmd
flowchart TD
  Missing --> Source
```
''',
        imageResolver: (_) async => null,
        diagramRenderer: (_, _) => throw StateError('no SVG'),
      );
      expect(result.html, contains('[Image: Missing]'));
      expect(result.html, contains('language-mmd'));
      expect(result.plainText, contains('Missing --> Source'));
      expect(result.html, isNot(contains('src="missing.jpg"')));
    },
  );

  test('formatted table embeds images inside their original cells', () async {
    final result = await markdownRichClipboardTableWithMedia(
      rows: const <MarkdownRichTableRow>[
        MarkdownRichTableRow(cells: ['Label', 'Picture'], isHeader: true),
        MarkdownRichTableRow(
          cells: ['One', '![logo](logo.png)'],
          isHeader: false,
        ),
      ],
      imageResolver: (_) async => image,
    );
    expect(result.html, contains('<table style='));
    expect(result.html, contains('<th style='));
    expect(result.html, contains('<td style='));
    expect(result.html, contains('src="data:image/png;base64,'));
    expect(result.plainText, contains('[Image: logo]'));
  });

  test(
    'rejects invalid MIME, oversized images, and oversized document media',
    () async {
      final wrongMime = MarkdownClipboardImage(
        bytes: imageBytes,
        mimeType: 'image/svg+xml',
      );
      final result = await markdownRichClipboardDocumentWithMedia(
        markdown: '![Invalid](pic.svg)',
        imageResolver: (_) async => wrongMime,
      );
      expect(result.html, contains('[Image: Invalid]'));

      final oversized = MarkdownClipboardImage(
        bytes: Uint8List(markdownClipboardMaxImageBytes + 1),
        mimeType: 'image/png',
      );
      final large = await markdownRichClipboardDocumentWithMedia(
        markdown: '![Big](big.png)',
        imageResolver: (_) async => oversized,
      );
      expect(large.html, contains('[Image: Big]'));
    },
  );

  test(
    'image loader resolves local PNG securely and does not accept file URI',
    () async {
      final folder = await Directory.systemTemp.createTemp('alera-rich-media-');
      addTearDown(() async => folder.delete(recursive: true));
      final assets = Directory('${folder.path}/docs/images');
      await assets.create(recursive: true);
      await File('${assets.path}/test.png').writeAsBytes(imageBytes);
      final loader = MarkdownClipboardImageLoader(
        workspacePath: folder.path,
        markdownPath: 'docs/guide.md',
      );
      final local = await loader.load('images/test.png');
      expect(local?.mimeType, 'image/png');
      expect(local?.bytes, imageBytes);
      expect(await loader.load('../escape.png'), isNull);
      expect(await loader.load('file:///etc/passwd'), isNull);
      expect(await loader.load('http://127.0.0.1/test.png'), isNull);
    },
  );

  test('rasterizes a Mermaid SVG to real PNG bytes', () async {
    final png = await markdownClipboardRasterizeSvg(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 60">'
      '<rect x="2" y="2" width="96" height="56" fill="red" />'
      '</svg>',
    );
    expect(png?.mimeType, 'image/png');
    expect(png?.bytes.take(4).toList(), [137, 80, 78, 71]);
  });

  test('converts GIF images and UTF-8 SVG graphics to portable PNG', () async {
    final gif = Uint8List.fromList(
      img.encodeGif(img.Image(width: 3, height: 2)),
    );
    final bitmap = await markdownClipboardImageFromBytes(gif);
    expect(bitmap?.mimeType, 'image/png');
    expect(bitmap?.bytes.take(4).toList(), [137, 80, 78, 71]);

    final svg = utf8.encode(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 50 25">'
      '<title>臺灣示意圖</title>'
      '<rect width="50" height="25" fill="#333333"/>'
      '</svg>',
    );
    final chart = await markdownClipboardImageFromBytes(
      Uint8List.fromList(svg),
    );
    expect(chart?.mimeType, 'image/png');
    expect(chart?.bytes.take(4).toList(), [137, 80, 78, 71]);
  });

  test('copy one PNG uses native bitmap clipboard rather than HTML', () async {
    String? method;
    Uint8List? copied;
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(MarkdownRichClipboard.channel, (
      call,
    ) async {
      method = call.method;
      copied = call.arguments as Uint8List;
      return null;
    });
    addTearDown(() {
      messenger.setMockMethodCallHandler(MarkdownRichClipboard.channel, null);
    });
    await const MarkdownRichClipboard().copyPng(imageBytes);
    expect(method, 'setImagePng');
    expect(copied, imageBytes);
  });
}
