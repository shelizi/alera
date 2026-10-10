import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:test/test.dart';

void main() {
  test('directory list parses page cursor, folders and file sizes', () {
    final page = MobileWorkspaceDirectoryPage.fromJson(<String, Object?>{
      'items': <Object?>[
        <String, Object?>{
          'name': 'src',
          'relativePath': 'src',
          'isDirectory': true,
          'sizeBytes': 0,
        },
        <String, Object?>{
          'name': 'main.dart',
          'relativePath': 'src/main.dart',
          'isDirectory': false,
          'sizeBytes': 42,
        },
      ],
      'hasMore': true,
      'nextOffset': 2,
    });
    expect(page.items, hasLength(2));
    expect(page.items.first.isDirectory, isTrue);
    expect(page.items.last.relativePath, 'src/main.dart');
    expect(page.items.last.sizeBytes, 42);
    expect(page.hasMore, isTrue);
    expect(page.nextOffset, 2);
  });

  test('invalid directory entry without a path is rejected', () {
    expect(
      () => MobileWorkspaceDirectoryPage.fromJson(<String, Object?>{
        'items': <Object?>[
          <String, Object?>{'name': 'bad', 'isDirectory': true},
        ],
      }),
      throwsFormatException,
    );
  });
}
