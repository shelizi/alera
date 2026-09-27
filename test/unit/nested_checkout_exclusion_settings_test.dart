import 'package:alera/src/features/language_intelligence/infra/nested_checkout_exclusion_settings.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  final windows = p.Context(style: p.Style.windows);
  final posix = p.Context(style: p.Style.posix);

  Map<String, Object?> sections(
    String providerId, {
    required p.Context context,
    required String root,
    required List<String> nested,
  }) => nestedCheckoutExclusionSections(
    providerId: providerId,
    workspaceRoot: root,
    nestedCheckouts: nested,
    pathContext: context,
  );

  test('no nested checkouts leaves every server with its own settings', () {
    expect(
      sections(
        'dart.analysis-server',
        context: posix,
        root: '/repo',
        nested: const <String>[],
      ),
      isEmpty,
    );
  });

  test('servers that never index nested checkouts get nothing', () {
    for (final id in <String>[
      'rust.rust-analyzer',
      'go.gopls',
      'python.pyrefly',
      'typescript-javascript.typescript-language-server',
      'csharp.csharp-ls',
    ]) {
      expect(
        sections(id, context: posix, root: '/repo', nested: ['/repo/.wt/a']),
        isEmpty,
        reason: id,
      );
    }
  });

  test('dart excludes absolute folders', () {
    expect(
      sections(
        'dart.analysis-server',
        context: windows,
        root: r'E:\repo',
        nested: [r'E:\repo\.worktrees\a'],
      ),
      {
        'dart': {
          'analysisExcludedFolders': [r'E:\repo\.worktrees\a'],
        },
      },
    );
  });

  test('pyright keeps its default excludes alongside the checkouts', () {
    expect(
      sections(
        'python.pyright',
        context: posix,
        root: '/repo',
        nested: ['/repo/wt/a'],
      ),
      {
        'python': {
          'analysis': {
            'exclude': [
              '**/node_modules',
              '**/__pycache__',
              '**/.*',
              '/repo/wt/a',
            ],
          },
        },
      },
    );
  });

  test('ty and intelephense use forward-slash paths relative to the root', () {
    for (final (context, root, nested) in [
      (windows, r'E:\repo', r'E:\repo\.worktrees\a'),
      (posix, '/repo', '/repo/.worktrees/a'),
    ]) {
      expect(
        sections('python.ty', context: context, root: root, nested: [nested]),
        {
          'ty': {
            'configuration': {
              'src': {
                'exclude': ['.worktrees/a'],
              },
            },
          },
        },
      );
      final php = sections(
        'php.intelephense',
        context: context,
        root: root,
        nested: [nested],
      );
      final exclude =
          ((php['intelephense']! as Map)['files'] as Map)['exclude'] as List;
      expect(exclude, contains('**/node_modules/**'));
      expect(exclude.last, '**/.worktrees/a/**');
    }
  });
}
