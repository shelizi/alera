import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/application/mobile_workspace_image_preview.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const signature = <int>[137, 80, 78, 71, 13, 10, 26, 10];

  MobileWorkspaceFileRange page({
    required int offset,
    required List<int> bytes,
    required int total,
    String path = 'assets/preview.png',
    String mime = 'image/png',
    bool isText = false,
  }) => MobileWorkspaceFileRange(
    relativePath: path,
    offset: offset,
    nextOffset: offset + bytes.length,
    totalBytes: total,
    mimeType: mime,
    isText: isText,
    bytes: bytes,
  );

  test('binary preview assembles signed image ranges without JS or URLs', () async {
    final offsets = <int>[];
    final result = await loadWorkspaceImagePreview((offset, length) async {
      offsets.add(offset);
      final bytes = offset == 0 ? signature.sublist(0, 4) : signature.sublist(4);
      return page(offset: offset, bytes: bytes, total: 8);
    });
    expect(offsets, <int>[0, 4]);
    expect(result.bytes, signature);
    expect(result.relativePath, 'assets/preview.png');
    expect(result.mimeType, 'image/png');
  });

  test('SVG and PDF are never previewed as executable image content', () async {
    expect(isSupportedMobileWorkspaceImageMime('image/svg+xml'), isFalse);
    expect(isSupportedMobileWorkspaceImageMime('application/pdf'), isFalse);
    expect(isSupportedMobileWorkspaceImageMime('image/png'), isTrue);
    await expectLater(
      loadWorkspaceImagePreview((offset, length) async => page(
        offset: offset,
        bytes: signature,
        total: 8,
        mime: 'image/svg+xml',
      )),
      throwsFormatException,
    );
  });

  test('image preview rejects oversized or mislabeled content', () async {
    await expectLater(
      loadWorkspaceImagePreview((offset, length) async => page(
        offset: offset,
        bytes: signature,
        total: maxMobileWorkspaceImagePreviewBytes + 1,
      )),
      throwsFormatException,
    );
    await expectLater(
      loadWorkspaceImagePreview((offset, length) async => page(
        offset: offset,
        bytes: <int>[60, 115, 99, 114, 105, 112, 116, 62],
        total: 8,
      )),
      throwsFormatException,
    );
  });

  test('image preview rejects mismatched chunk path or MIME', () async {
    await expectLater(
      loadWorkspaceImagePreview((offset, length) async => page(
        offset: offset,
        bytes: offset == 0 ? signature.sublist(0, 4) : signature.sublist(4),
        total: 8,
        path: offset == 0 ? 'assets/a.png' : 'assets/b.png',
      )),
      throwsFormatException,
    );
    await expectLater(
      loadWorkspaceImagePreview((offset, length) async => page(
        offset: offset,
        bytes: offset == 0 ? signature.sublist(0, 4) : signature.sublist(4),
        total: 8,
        mime: offset == 0 ? 'image/png' : 'image/jpeg',
      )),
      throwsFormatException,
    );
  });
}
