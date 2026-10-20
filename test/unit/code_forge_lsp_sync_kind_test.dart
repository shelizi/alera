import 'package:code_forge/code_forge.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'real CodeForge initialization capability parser negotiates sync mode',
    () {
      expect(negotiatedLspTextDocumentSyncKind(null), isNull);
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{}),
        isNull,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': 2,
        }),
        2,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': <String, dynamic>{
            'openClose': true,
            'change': 2,
            'save': <String, dynamic>{'includeText': true},
          },
        }),
        2,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': 1,
        }),
        1,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': <String, dynamic>{'openClose': true},
        }),
        isNull,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': 42,
        }),
        isNull,
      );
      expect(
        negotiatedLspTextDocumentSyncKind(const <String, dynamic>{
          'textDocumentSync': 2,
        }, forceFullDocumentSync: true),
        1,
      );
    },
  );
}
