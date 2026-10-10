import 'dart:convert';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_text_preview.dart';
import 'package:test/test.dart';

void main() {
  test(
    'only a complete, strict UTF-8 snapshot has an editable digest',
    () async {
      final source = utf8.encode('hello');
      final complete = await loadWorkspaceTextPreview(
        (offset, length) async => _page(
          offset: offset,
          bytes: source.sublist(offset),
          total: source.length,
        ),
      );
      expect(
        complete.originalSha256,
        '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
      );
      final truncated = await loadWorkspaceTextPreview(
        (offset, length) async => _page(
          offset: offset,
          bytes: source.sublist(offset, offset + length),
          total: source.length,
        ),
        maxBytes: 2,
      );
      expect(truncated.originalSha256, isNull);

      final malformed = await loadWorkspaceTextPreview(
        (offset, length) async =>
            _page(offset: offset, bytes: [0xc3, 0x28], total: 2),
      );
      expect(malformed.isText, isTrue);
      expect(malformed.originalSha256, isNull);
    },
  );

  test('a complete file larger than edit limit remains read-only', () async {
    final bytes = List<int>.filled(maxMobileWorkspaceTextEditBytes + 1, 65);
    final snapshot = await loadWorkspaceTextPreview((offset, length) async {
      final end = (offset + length).clamp(0, bytes.length);
      return _page(
        offset: offset,
        bytes: bytes.sublist(offset, end),
        total: bytes.length,
      );
    });
    expect(snapshot.truncated, isFalse);
    expect(snapshot.originalSha256, isNull);
  });

  test('reassembles split UTF-8 characters before decoding', () async {
    final raw = utf8.encode('a€');
    final preview = await loadWorkspaceTextPreview((offset, length) async {
      final end = offset == 0 ? 2 : raw.length;
      return _page(
        offset: offset,
        bytes: raw.sublist(offset, end),
        total: raw.length,
      );
    });
    expect(preview.text, 'a€');
    expect(preview.isText, isTrue);
    expect(preview.truncated, isFalse);
  });

  test(
    'stops at preview byte budget and never silently claims full file',
    () async {
      final preview = await loadWorkspaceTextPreview(
        (offset, length) async => _page(
          offset: offset,
          bytes: utf8.encode('abcdef').sublist(offset, offset + length),
          total: 6,
        ),
        maxBytes: 3,
      );
      expect(preview.text, 'abc');
      expect(preview.truncated, isTrue);
      expect(preview.totalBytes, 6);
    },
  );

  test('does not decode binary file contents', () async {
    final preview = await loadWorkspaceTextPreview(
      (offset, length) async =>
          _page(offset: offset, bytes: [0, 255], total: 2, isText: false),
    );
    expect(preview.isText, isFalse);
    expect(preview.text, isEmpty);
  });

  test('rejects malformed or noncontiguous remote ranges', () async {
    await expectLater(
      loadWorkspaceTextPreview(
        (offset, length) async => _page(offset: 9, bytes: [65], total: 1),
      ),
      throwsFormatException,
    );
  });
}

MobileWorkspaceFileRange _page({
  required int offset,
  required List<int> bytes,
  required int total,
  bool isText = true,
}) => MobileWorkspaceFileRange(
  relativePath: 'src/main.dart',
  offset: offset,
  nextOffset: offset + bytes.length,
  totalBytes: total,
  mimeType: 'text/plain',
  isText: isText,
  bytes: bytes,
);
