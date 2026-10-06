import 'dart:io' show Directory, File, FileSystemException, Link;

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_uri_policy.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_viewer_images.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_viewer_surface.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/shared/infra/uri/external_uri_launcher.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('markdown viewer link policy only accepts web URLs with hosts', () {
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('https://example.com/docs')),
      isTrue,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('http://example.com')),
      isTrue,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('file:///tmp/readme.md')),
      isFalse,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('mailto:test@example.com')),
      isFalse,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('vscode://file/foo')),
      isFalse,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.parse('https:///missing-host')),
      isFalse,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.tryParse('docs/readme.md')),
      isFalse,
    );
    expect(
      isSupportedMarkdownViewerLinkUri(Uri.tryParse('not a uri')),
      isFalse,
    );
  });

  test('markdown viewer image policy resolves only safe image sources', () {
    expect(
      isSupportedMarkdownViewerRemoteImageUri(
        Uri.parse('https://example.com/diagram.png'),
      ),
      isTrue,
    );
    expect(
      isSupportedMarkdownViewerRemoteImageUri(
        Uri.parse('http://example.com/diagram.png'),
      ),
      isTrue,
    );
    expect(
      isSupportedMarkdownViewerRemoteImageUri(
        Uri.parse('file:///tmp/diagram.png'),
      ),
      isFalse,
    );
    expect(
      resolveWorkspaceMarkdownImagePath(
        markdownPath: 'docs/readme.md',
        rawImageUrl: './images/diagram.png',
      ),
      'docs/images/diagram.png',
    );
    expect(
      resolveWorkspaceMarkdownImagePath(
        markdownPath: 'docs/guides/readme.md',
        rawImageUrl: '../assets/diagram.png',
      ),
      'docs/assets/diagram.png',
    );
    expect(
      resolveWorkspaceMarkdownImagePath(
        markdownPath: 'docs/readme.md',
        rawImageUrl: '../../secret.png',
      ),
      isNull,
    );
    expect(
      resolveWorkspaceMarkdownImagePath(
        markdownPath: 'docs/readme.md',
        rawImageUrl: r'C:\secret.png',
      ),
      isNull,
    );
    expect(
      resolveWorkspaceMarkdownImagePath(
        markdownPath: 'docs/readme.md',
        rawImageUrl: 'file:///tmp/secret.png',
      ),
      isNull,
    );
  });

  test('markdown viewer image builder creates explicit non-local widgets', () {
    final remoteImage = buildMarkdownViewerImage(
      workspacePath: '/repo/alera',
      markdownPath: 'docs/readme.md',
      imageUrl: 'https://example.com/diagram.png',
    ) as Image;
    expect(remoteImage.image, isA<NetworkImage>());
    expect(
      (remoteImage.image as NetworkImage).url,
      'https://example.com/diagram.png',
    );

    final blockedImage = buildMarkdownViewerImage(
      workspacePath: '/repo/alera',
      markdownPath: 'docs/readme.md',
      imageUrl: 'file:///tmp/secret.png',
    );
    expect(blockedImage, isNot(isA<Image>()));
  });

  test('markdown viewer dirty content guard skips unchanged previews', () {
    expect(
      shouldUpdateMarkdownViewerDirtyContent(
        currentContent: '# Dirty',
        dirtyEditorContent: '# Dirty',
        loading: false,
        loadError: null,
        usingDirtyEditorContent: true,
      ),
      isFalse,
    );
    expect(
      shouldUpdateMarkdownViewerDirtyContent(
        currentContent: '# Dirty',
        dirtyEditorContent: '# Changed',
        loading: false,
        loadError: null,
        usingDirtyEditorContent: true,
      ),
      isTrue,
    );
    expect(
      shouldUpdateMarkdownViewerDirtyContent(
        currentContent: '# Dirty',
        dirtyEditorContent: '# Dirty',
        loading: true,
        loadError: null,
        usingDirtyEditorContent: true,
      ),
      isTrue,
    );
    expect(
      shouldUpdateMarkdownViewerDirtyContent(
        currentContent: '# Dirty',
        dirtyEditorContent: '# Dirty',
        loading: false,
        loadError: StateError('failed'),
        usingDirtyEditorContent: true,
      ),
      isTrue,
    );
    expect(
      shouldUpdateMarkdownViewerDirtyContent(
        currentContent: '# Dirty',
        dirtyEditorContent: '# Dirty',
        loading: false,
        loadError: null,
        usingDirtyEditorContent: false,
      ),
      isTrue,
    );
  });

  test('markdown preview search matches case-insensitively', () {
    expect(
      markdownViewerSearchMatchOffsets('Alpha beta alpha ALPHA', 'alpha'),
      const <int>[0, 11, 17],
    );
    expect(markdownViewerSearchMatchOffsets('Alpha', '   '), isEmpty);
    expect(markdownViewerSearchMatchOffsets('', 'alpha'), isEmpty);
  });

  test('markdown preview recognizes Mermaid fence aliases', () {
    expect(isMarkdownMermaidFence('mermaid'), isTrue);
    expect(isMarkdownMermaidFence('Mermaid'), isTrue);
    expect(isMarkdownMermaidFence('mmd'), isTrue);
    expect(isMarkdownMermaidFence('merman'), isTrue);
    expect(isMarkdownMermaidFence('dart'), isFalse);
  });

  testWidgets('markdown preview visibly highlights search matches', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService(
      '# Alpha heading\n\nBeta alpha body',
    );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Search preview'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(EditableText).last, 'alpha');
    await tester.pumpAndSettle();

    expect(find.text('1/2'), findsOneWidget);
    final firstPass = _searchHighlightColors(tester, 'alpha');
    expect(
      firstPass.where((color) => color == AleraTokens.accentSubtle),
      hasLength(2),
    );

    await tester.tap(find.byTooltip('Next Match'));
    await tester.pumpAndSettle();
    expect(find.text('2/2'), findsOneWidget);
    final secondPass = _searchHighlightColors(tester, 'alpha');
    expect(
      secondPass.where((color) => color == AleraTokens.accentSubtle),
      hasLength(2),
    );
  });

  test('canonical local image resolver allows workspace files', () async {
    final tempRoot = await Directory.systemTemp.createTemp('alera-md-image-');
    addTearDown(() async {
      await tempRoot.delete(recursive: true);
    });
    final workspace = await Directory('${tempRoot.path}/workspace').create();
    final image = await File('${workspace.path}/docs/images/diagram.png')
        .create(recursive: true);
    await image.writeAsBytes(const <int>[0]);

    final resolved = await resolveWorkspaceMarkdownImageFilePath(
      workspacePath: workspace.path,
      markdownPath: 'docs/readme.md',
      rawImageUrl: './images/diagram.png',
    );

    expect(resolved, await image.resolveSymbolicLinks());
  });

  test('canonical local image resolver constrains symlink targets', () async {
    final tempRoot = await Directory.systemTemp.createTemp('alera-md-symlink-');
    addTearDown(() async {
      await tempRoot.delete(recursive: true);
    });
    final workspace = await Directory('${tempRoot.path}/workspace').create();
    final docs = await Directory('${workspace.path}/docs').create();
    final assets = await Directory('${workspace.path}/assets').create();
    final outside = await Directory('${tempRoot.path}/outside').create();
    final insideImage = await File('${assets.path}/inside.png').create();
    final outsideImage = await File('${outside.path}/outside.png').create();
    await insideImage.writeAsBytes(const <int>[1]);
    await outsideImage.writeAsBytes(const <int>[2]);

    final symlinksCreated =
        await _createSymlinkOrSkip(
          linkPath: '${docs.path}/inside-link.png',
          targetPath: insideImage.path,
        ) &&
        await _createSymlinkOrSkip(
          linkPath: '${docs.path}/outside-link.png',
          targetPath: outsideImage.path,
        ) &&
        await _createSymlinkOrSkip(
          linkPath: '${docs.path}/broken-link.png',
          targetPath: '${outside.path}/missing.png',
        );
    if (!symlinksCreated) {
      return;
    }

    expect(
      await resolveWorkspaceMarkdownImageFilePath(
        workspacePath: workspace.path,
        markdownPath: 'docs/readme.md',
        rawImageUrl: './inside-link.png',
      ),
      await insideImage.resolveSymbolicLinks(),
    );
    expect(
      await resolveWorkspaceMarkdownImageFilePath(
        workspacePath: workspace.path,
        markdownPath: 'docs/readme.md',
        rawImageUrl: './outside-link.png',
      ),
      isNull,
    );
    expect(
      await resolveWorkspaceMarkdownImageFilePath(
        workspacePath: workspace.path,
        markdownPath: 'docs/readme.md',
        rawImageUrl: './broken-link.png',
      ),
      isNull,
    );
  });

  testWidgets('renders dirty editor content instead of saved disk content', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');
    registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Disk', displayContent: '# Disk'),
      )
      ..updateCurrentText('# Dirty');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('Dirty'), findsOneWidget);
    expect(find.textContaining('Disk'), findsNothing);
    expect(service.reads, isEmpty);
  });

  testWidgets('reads disk content when a matching editor buffer is clean', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');
    registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Cached', displayContent: '# Cached'),
      );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('Disk'), findsOneWidget);
    expect(find.textContaining('Cached'), findsNothing);
    expect(service.reads, const <String>['docs/readme.md']);
  });

  testWidgets('falls back to disk content when no editor buffer exists', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('Disk'), findsOneWidget);
    expect(service.reads, const <String>['docs/readme.md']);
  });

  testWidgets('searches within markdown preview and navigates matches', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService(
      '# Alpha\n\nBeta alpha\n\nALPHA final',
    );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Search preview'));
    await tester.pumpAndSettle();
    expect(find.text('Find in Preview'), findsOneWidget);

    await tester.enterText(find.byType(EditableText).last, 'alpha');
    await tester.pumpAndSettle();
    expect(find.text('1/3'), findsOneWidget);

    await tester.tap(find.byTooltip('Next Match'));
    await tester.pumpAndSettle();
    expect(find.text('2/3'), findsOneWidget);

    await tester.tap(find.byTooltip('Close Search'));
    await tester.pumpAndSettle();
    expect(find.text('Find in Preview'), findsNothing);
  });

  testWidgets('restores scroll position after switching away and back', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService(
      List<String>.generate(
        120,
        (index) =>
            'Paragraph $index has enough markdown preview text to make the '
            'document scroll vertically.',
      ).join('\n\n'),
    );
    final showMarkdown = ValueNotifier<bool>(true);
    addTearDown(showMarkdown.dispose);

    await tester.pumpWidget(
      _switchableSurface(
        registry: registry,
        workspaceFiles: service,
        showMarkdown: showMarkdown,
      ),
    );
    await tester.pumpAndSettle();

    await tester.drag(
      find.byType(SingleChildScrollView),
      const Offset(0, -600),
    );
    await tester.pumpAndSettle();
    final offsetBeforeSwitch = _markdownScrollOffset(tester);
    expect(offsetBeforeSwitch, greaterThan(0));

    showMarkdown.value = false;
    await tester.pump();
    expect(find.byType(SingleChildScrollView), findsNothing);

    showMarkdown.value = true;
    await tester.pumpAndSettle();

    expect(_markdownScrollOffset(tester), closeTo(offsetBeforeSwitch, 0.5));
  });

  testWidgets(
    'keeps horizontal overflow controls visible in markdown preview',
    (tester) async {
      final registry = EditorSessionRegistry();
      final service = _FakeWorkspaceFileService('''
```text
${List<String>.filled(220, 'wide').join('_')}
```

\\[
${List<String>.filled(80, r'\frac{a}{b}').join(' + ')}
\\]
''');

      await tester.pumpWidget(
        _surface(registry: registry, workspaceFiles: service),
      );
      await tester.pumpAndSettle();

      final scrollbarTheme = tester.widget<ScrollbarTheme>(
        find.byType(ScrollbarTheme),
      );
      expect(
        scrollbarTheme.data.thumbVisibility?.resolve(const <WidgetState>{}),
        isTrue,
      );
      expect(
        scrollbarTheme.data.trackVisibility?.resolve(const <WidgetState>{}),
        isTrue,
      );
      expect(scrollbarTheme.data.thickness?.resolve(const <WidgetState>{}), 8);
      expect(
        scrollbarTheme.data.thumbColor?.resolve(const <WidgetState>{}),
        AleraTokens.foregroundMuted,
      );

      final codeScrollbar = tester.widget<Scrollbar>(
        find.byKey(const ValueKey<String>('markdown-code-x-scrollbar')),
      );
      expect(codeScrollbar.thumbVisibility, isTrue);
      expect(codeScrollbar.trackVisibility, isTrue);
      expect(codeScrollbar.thickness, 8);
      expect(codeScrollbar.interactive, isTrue);
      expect(codeScrollbar.scrollbarOrientation, ScrollbarOrientation.bottom);

      final horizontalPositions = tester
          .stateList<ScrollableState>(find.byType(Scrollable))
          .map((state) => state.position)
          .where((position) => position.axis == Axis.horizontal)
          .toList(growable: false);
      expect(horizontalPositions, isNotEmpty);
      expect(
        horizontalPositions.any((position) => position.maxScrollExtent > 0),
        isTrue,
      );
    },
  );

  testWidgets(
    'table scrollbar leaves a gutter and resets horizontal position',
    (tester) async {
      final registry = EditorSessionRegistry();
      final longCell = List<String>.filled(80, 'wide').join('_');
      final service = _FakeWorkspaceFileService('''
| First | Second |
| --- | --- |
| row one | $longCell |
| final row | $longCell |
''');
      final showMarkdown = ValueNotifier<bool>(true);
      addTearDown(showMarkdown.dispose);

      await tester.pumpWidget(
        _switchableSurface(
          registry: registry,
          workspaceFiles: service,
          showMarkdown: showMarkdown,
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.byKey(const ValueKey<String>('markdown-table-x-scrollbar')),
        findsOneWidget,
      );
      final horizontalView = tester.widget<SingleChildScrollView>(
        find.byKey(const ValueKey<String>('markdown-table-x-scrollview')),
      );
      expect(
        horizontalView.padding,
        const EdgeInsets.only(bottom: AleraTokens.space16),
      );

      ScrollPosition position() {
        final scrollable = find.descendant(
          of: find.byKey(const ValueKey<String>('markdown-table-x-scrollview')),
          matching: find.byType(Scrollable),
        );
        return tester.state<ScrollableState>(scrollable).position;
      }

      expect(position().maxScrollExtent, greaterThan(0));
      expect(position().pixels, position().minScrollExtent);
      await tester.drag(
        find.byKey(const ValueKey<String>('markdown-table-x-scrollview')),
        const Offset(-300, 0),
      );
      await tester.pumpAndSettle();
      expect(position().pixels, greaterThan(0));

      showMarkdown.value = false;
      await tester.pump();
      showMarkdown.value = true;
      await tester.pumpAndSettle();
      expect(position().pixels, position().minScrollExtent);
    },
  );

  testWidgets('renders Mermaid fences as SVG in markdown preview', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('''
```mermaid
flowchart TD
  A --> B
```
''');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(service.mermanSources.single.trimRight(), 'flowchart TD\n  A --> B');
    expect(
      find.byKey(
        const ValueKey<String>('markdown-mermaid-svg:docs/readme.md:0'),
      ),
      findsNothing,
    );
    expect(
      find.byWidgetPredicate(
        (widget) =>
            widget is SvgPicture &&
            widget.key is ValueKey<String> &&
            ((widget.key as ValueKey<String>).value).startsWith(
              'markdown-mermaid-svg:',
            ),
      ),
      findsOneWidget,
    );
    expect(find.text('Mermaid diagram cannot be rendered'), findsNothing);
  });
  testWidgets('wide preview blocks use isolated selectable regions', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('''
Paragraph text remains selectable.

| A | B |
| --- | --- |
| one | two |

```text
${List<String>.filled(120, 'selectable').join('_')}
```
''');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.byType(SelectionArea), findsNWidgets(3));
  });

  testWidgets('refresh reloads disk content while the editor buffer is clean', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk v1');
    registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Disk v1', displayContent: '# Disk v1'),
      );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('Disk v1'), findsOneWidget);

    service.content = '# Disk v2';
    await tester.tap(find.byTooltip('Refresh preview'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Disk v2'), findsOneWidget);
    expect(find.textContaining('Disk v1'), findsNothing);
    expect(service.reads, const <String>['docs/readme.md', 'docs/readme.md']);
  });

  testWidgets('updates an open preview when editor content changes', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('Disk'), findsOneWidget);

    registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Disk', displayContent: '# Disk'),
      )
      ..updateCurrentText('# Dirty');
    await tester.pumpAndSettle();

    expect(find.textContaining('Dirty'), findsOneWidget);
    expect(find.textContaining('Disk'), findsNothing);
    expect(service.reads, const <String>['docs/readme.md']);
  });

  testWidgets('debounces rapid editor updates and renders only the latest', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');
    final document = registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Disk', displayContent: '# Disk'),
      );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    final halfDebounce = Duration(
      milliseconds:
          workspaceMarkdownViewerEditorUpdateDebounce.inMilliseconds ~/ 2,
    );

    document.updateCurrentText('# First');
    await tester.pump(halfDebounce);
    expect(find.textContaining('Disk'), findsOneWidget);
    expect(find.textContaining('First'), findsNothing);

    document.updateCurrentText('# Latest');
    await tester.pump(
      workspaceMarkdownViewerEditorUpdateDebounce - halfDebounce,
    );
    expect(find.textContaining('Latest'), findsNothing);

    await tester.pump(workspaceMarkdownViewerEditorUpdateDebounce);
    expect(find.textContaining('Latest'), findsOneWidget);
    expect(find.textContaining('First'), findsNothing);
  });

  testWidgets('ignores dirty editor changes from unrelated files', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService('# Disk');
    registry.documentFor('editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/readme.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Disk', displayContent: '# Disk'),
      )
      ..updateCurrentText('# Dirty preview');

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await tester.pumpAndSettle();

    registry.documentFor('other-editor-tab')
      ..attachFile(workspacePath: '/repo/alera', relativePath: 'docs/other.md')
      ..acceptLoaded(
        _editorFile(rawContent: '# Other', displayContent: '# Other'),
      )
      ..updateCurrentText('# Other dirty');
    await tester.pumpAndSettle();

    expect(find.textContaining('Dirty preview'), findsOneWidget);
    expect(find.textContaining('Other dirty'), findsNothing);
    expect(service.reads, isEmpty);
  });

  testWidgets('does not launch non-web markdown links', (tester) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService(
      '[Local file](file:///tmp/readme.md)',
    );
    final launcher = _FakeExternalUriLauncher();

    await tester.pumpWidget(
      _surface(
        registry: registry,
        workspaceFiles: service,
        externalUriLauncher: launcher,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Local file'));
    await tester.pumpAndSettle();

    expect(launcher.opened, isEmpty);
  });

  testWidgets('blocks unsupported markdown image sources in the preview', (
    tester,
  ) async {
    final registry = EditorSessionRegistry();
    final service = _FakeWorkspaceFileService(
      '![32x16](file:///tmp/secret.png)',
    );

    await tester.pumpWidget(
      _surface(registry: registry, workspaceFiles: service),
    );
    await _pumpLoadedMarkdown(tester);

    expect(find.byType(Image), findsNothing);
    expect(find.byIcon(AleraIcons.imageError), findsOneWidget);
  });
}

List<Color?> _searchHighlightColors(WidgetTester tester, String query) {
  return <Color?>[
    for (final richText in tester.widgetList<RichText>(find.byType(RichText)))
      for (final span in _textSpanLeaves(richText.text))
        if (span.text?.toLowerCase() == query.toLowerCase())
          span.style?.backgroundColor,
  ];
}

Iterable<TextSpan> _textSpanLeaves(InlineSpan span) sync* {
  if (span is! TextSpan) {
    return;
  }
  if (span.text != null && span.text!.isNotEmpty) {
    yield span;
  }
  for (final child in span.children ?? const <InlineSpan>[]) {
    yield* _textSpanLeaves(child);
  }
}

Future<void> _pumpLoadedMarkdown(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 1));
}

Future<bool> _createSymlinkOrSkip({
  required String linkPath,
  required String targetPath,
}) async {
  try {
    await Link(linkPath).create(targetPath);
    return true;
  } on FileSystemException catch (error) {
    markTestSkipped('Symlink creation failed: $error');
    return false;
  }
}

Widget _surface({
  required EditorSessionRegistry registry,
  required WorkspaceFileService workspaceFiles,
  ExternalUriLauncher? externalUriLauncher,
  Workspace? workspace,
}) {
  return ProviderScope(
    overrides: [
      editorSessionRegistryProvider.overrideWithValue(registry),
      workspaceFileServiceProvider.overrideWithValue(workspaceFiles),
      if (externalUriLauncher != null)
        externalUriLauncherProvider.overrideWithValue(externalUriLauncher),
    ],
    child: MaterialApp(
      home: Scaffold(
        body: WorkspaceMarkdownViewerSurface(
          workspace: workspace ?? _workspace(),
          tab: _tab(),
          onOpenEditorTab: (_) {},
        ),
      ),
    ),
  );
}

Widget _switchableSurface({
  required EditorSessionRegistry registry,
  required WorkspaceFileService workspaceFiles,
  required ValueNotifier<bool> showMarkdown,
}) {
  return ProviderScope(
    overrides: [
      editorSessionRegistryProvider.overrideWithValue(registry),
      workspaceFileServiceProvider.overrideWithValue(workspaceFiles),
    ],
    child: MaterialApp(
      home: Scaffold(
        body: ValueListenableBuilder<bool>(
          valueListenable: showMarkdown,
          builder: (context, visible, _) {
            if (!visible) {
              return const Center(child: Text('Other tab'));
            }
            return WorkspaceMarkdownViewerSurface(
              workspace: _workspace(),
              tab: _tab(),
              onOpenEditorTab: (_) {},
            );
          },
        ),
      ),
    ),
  );
}

double _markdownScrollOffset(WidgetTester tester) {
  final scrollable = find.descendant(
    of: find.byType(SingleChildScrollView),
    matching: find.byType(Scrollable),
  );
  return tester.state<ScrollableState>(scrollable).position.pixels;
}

Workspace _workspace({String path = '/repo/alera'}) {
  final now = DateTime(2026);
  return Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'alera',
    path: path,
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}

WorkspaceTabRecord _tab() {
  final now = DateTime(2026);
  return WorkspaceTabRecord(
    id: 'preview-tab',
    workspaceId: 'workspace-1',
    kind: .markdownViewer,
    title: 'readme.md preview',
    createdAt: now,
    updatedAt: now,
    payload: const <String, Object?>{
      workspaceTabFilePathPayloadKey: 'docs/readme.md',
    },
  );
}

native.WorkspaceEditorTextFile _editorFile({
  required String rawContent,
  required String displayContent,
}) {
  return native.WorkspaceEditorTextFile(
    rawContent: rawContent,
    displayContent: displayContent,
    contentToken: 'editor-token',
    modifiedMillis: 0,
    size: .from(rawContent.length),
    encoding: native.WorkspaceTextEncoding.utf8,
  );
}

class _FakeWorkspaceFileService(var String content)
    extends WorkspaceFileService {
  final List<String> reads = <String>[];
  int mermaidRenders = 0;
  final List<String> mermanSources = <String>[];

  @override
  Future<native.WorkspaceTextFile> readTextFile({
    required String workspacePath,
    required String relativePath,
  }) async {
    reads.add(relativePath);
    return native.WorkspaceTextFile(
      content: content,
      contentToken: 'disk-token',
      modifiedMillis: 0,
      size: .from(content.length),
    );
  }

  @override
  Future<String> renderMermanSource({
    required String source,
    required String diagramId,
  }) async {
    mermaidRenders += 1;
    mermanSources.add(source);
    return '<svg viewBox="0 0 10 10">'
        '<rect x="1" y="1" width="8" height="8" />'
        '</svg>';
  }
}

class _FakeExternalUriLauncher implements ExternalUriLauncher {
  final List<Uri> opened = <Uri>[];

  @override
  Future<void> open(Uri uri) async {
    opened.add(uri);
  }
}
