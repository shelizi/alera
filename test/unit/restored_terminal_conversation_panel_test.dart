import 'package:alera/src/features/workbench/presentation/terminal_restored_conversation_panel.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final good = <String, Object?>{
    'readOnly': true,
    'source': 'nativeProvider',
    'messages': <Map<String, Object?>>[
      <String, Object?>{'role': 'user', 'text': '原始問題'},
      <String, Object?>{'role': 'assistant', 'text': '原始回答'},
    ],
    'hasMore': true,
    'nextBefore': 450,
  };

  test('restored history keeps actual user/assistant turns in order', () {
    final page = RestoredNativeConversationPage.parse(good, null);
    expect(page.messages.map((entry) => entry.role), ['user', 'assistant']);
    expect(page.messages.map((entry) => entry.text), ['原始問題', '原始回答']);
    expect(page.before, 450);
    final older = RestoredNativeConversationPage.parse(<String, Object?>{
      ...good,
      'nextBefore': 120,
    }, 450);
    expect(older.before, 120);
  });

  test('refuses tool text, forged sources and unsafe reverse cursor', () {
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'messages': [
          {'role': 'tool', 'text': 'secret'},
        ],
      }, null),
      throwsFormatException,
    );
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'source': 'unknown',
      }, null),
      throwsFormatException,
    );
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'readOnly': false,
      }, null),
      throwsFormatException,
    );
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'nextBefore': 451,
      }, 450),
      throwsFormatException,
    );
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'messages': [
          {'role': 'assistant', 'text': 'x' * 8001},
        ],
      }, null),
      throwsFormatException,
    );
  });

  test(
    'tool-only latest page retains bounded older cursor for manual browsing',
    () {
      final page = RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'messages': <Object?>[],
        'hasMore': true,
        'nextBefore': 94,
      }, null);
      expect(page.messages, isEmpty);
      expect(page.before, 94);
    },
  );

  test('bounds message count and does not pretend full pages exist', () {
    expect(
      () => RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'messages': List<Object?>.filled(61, <String, Object?>{
          'role': 'user',
          'text': 'message',
        }),
      }, null),
      throwsFormatException,
    );
    expect(
      RestoredNativeConversationPage.parse(<String, Object?>{
        ...good,
        'hasMore': false,
        'nextBefore': null,
      }, null).before,
      isNull,
    );
  });
}
