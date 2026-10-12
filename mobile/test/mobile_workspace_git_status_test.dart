import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:test/test.dart';

void main() {
  test('Git status parser preserves paths, statuses and truncation', () {
    final parsed = MobileWorkspaceGitStatus.fromJson(<String, Object?>{
      'items': <Object?>[
        <String, Object?>{
          'relativePath': 'src/main.dart',
          'status': 'modified',
        },
        <String, Object?>{'relativePath': 'new file.dart', 'status': 'added'},
      ],
      'truncated': true,
    });
    expect(parsed.items, hasLength(2));
    expect(parsed.items.first.relativePath, 'src/main.dart');
    expect(parsed.items.first.status, 'modified');
    expect(parsed.items.last.status, 'added');
    expect(parsed.truncated, isTrue);
  });

  test('Git status parser rejects missing file paths', () {
    expect(
      () => MobileWorkspaceGitStatus.fromJson(<String, Object?>{
        'items': <Object?>[
          <String, Object?>{'status': 'modified'},
        ],
      }),
      throwsFormatException,
    );
  });
}
