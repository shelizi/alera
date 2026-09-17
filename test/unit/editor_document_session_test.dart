import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native-backed document tracks dirty state by revision without text snapshots', () {
    final document = EditorDocumentSession();

    document.acceptNativeLoaded(
      encoding: native.WorkspaceTextEncoding.utf8,
      contentToken: '10:1',
      documentVersion: 7,
      tabSize: 4,
    );

    expect(document.nativeBacked, isTrue);
    expect(document.loadedRawText, isNull);
    expect(document.loadedText, isNull);
    expect(document.currentText, isNull);
    expect(document.isDirty, isFalse);

    expect(document.updateNativeDocumentVersion(8), isTrue);
    expect(document.isDirty, isTrue);
    expect(document.currentText, isNull);
  });

  test('native-backed save advances baseline without retaining full text', () {
    final document = EditorDocumentSession();
    document.acceptNativeLoaded(
      encoding: native.WorkspaceTextEncoding.utf8,
      contentToken: '10:1',
      documentVersion: 3,
      tabSize: 4,
    );
    document.updateNativeDocumentVersion(5);

    document.acceptNativeSaved(
      encoding: native.WorkspaceTextEncoding.utf8,
      contentToken: '12:2',
      savedDocumentVersion: 5,
      currentDocumentVersion: 6,
      tabSize: 4,
    );

    expect(document.nativeBacked, isTrue);
    expect(document.loadedRawText, isNull);
    expect(document.loadedText, isNull);
    expect(document.currentText, isNull);
    expect(document.loadedDocumentVersion, 5);
    expect(document.currentDocumentVersion, 6);
    expect(document.isDirty, isTrue);
    expect(document.contentToken, '12:2');
  });
}
