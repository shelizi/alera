import '../domain/language_id.dart';

abstract interface class LanguageDocumentSessionPort {
  Future<void> openDocument({
    required LanguageId language,
    required String path,
    required String text,
  });

  Future<void> replaceDocument({required String path, required String text});

  Future<void> saveDocument({required String path});

  Future<void> closeDocument({required String path});
}
