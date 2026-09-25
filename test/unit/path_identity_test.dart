import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  final windows = p.Context(style: p.Style.windows);
  final posix = p.Context(style: p.Style.posix);

  group('withoutWindowsPathPrefix', () {
    String plain(String path) =>
        withoutWindowsPathPrefix(path, pathContext: windows);

    test('returns the plain spelling of verbatim and device drive paths', () {
      expect(plain(r'\\?\E:\Work\Repo'), r'E:\Work\Repo');
      expect(plain(r'\\?\e:/work/repo'), 'e:/work/repo');
      expect(plain(r'\\.\C:\repo'), r'C:\repo');
      expect(plain(r'\\?\UNC\server\share\repo'), r'\\server\share\repo');
    });

    test('keeps prefixed paths that have no plain spelling', () {
      expect(plain(r'\\?\Volume{1234}\repo'), r'\\?\Volume{1234}\repo');
      expect(plain(r'\\.\pipe\alera'), r'\\.\pipe\alera');
      expect(
        hasUnresolvedWindowsPathPrefix(
          r'\\?\Volume{1234}\repo',
          pathContext: windows,
        ),
        isTrue,
      );
      expect(
        hasUnresolvedWindowsPathPrefix(r'\\?\E:\repo', pathContext: windows),
        isFalse,
      );
    });

    test('leaves plain and POSIX paths untouched', () {
      expect(plain(r'E:\Work\Repo'), r'E:\Work\Repo');
      expect(plain(r'\\server\share'), r'\\server\share');
      // On POSIX `\\?\` is ordinary filename text.
      expect(
        withoutWindowsPathPrefix(r'\\?\E:\Work', pathContext: posix),
        r'\\?\E:\Work',
      );
      expect(
        hasUnresolvedWindowsPathPrefix(r'\\?\Volume{1}', pathContext: posix),
        isFalse,
      );
    });
  });

  group('comparisons', () {
    test('treat verbatim, plain and lowercase spellings as one location', () {
      expect(
        isSamePath(r'\\?\E:\Work\Repo', r'e:\work\repo\', pathContext: windows),
        isTrue,
      );
      expect(
        isSamePath(r'\\?\E:\Work\Repo', r'E:\Work\Other', pathContext: windows),
        isFalse,
      );
      expect(
        comparablePath(r'\\?\E:\Work\.\Repo\..\Repo', pathContext: windows),
        r'E:\Work\Repo',
      );
    });

    test('contain paths across spellings without matching siblings', () {
      expect(
        isPathWithinOrSame(
          r'\\?\E:\Work\Repo',
          r'e:\work\repo\lib\main.dart',
          pathContext: windows,
        ),
        isTrue,
      );
      expect(
        isPathWithinOrSame(
          r'\\?\E:\Work\Repo',
          r'E:\Work\Repo',
          pathContext: windows,
        ),
        isTrue,
      );
      expect(
        isPathWithinOrSame(
          r'\\?\E:\Work\Repo',
          r'E:\Work\Repo-other\a.dart',
          pathContext: windows,
        ),
        isFalse,
      );
    });

    test('relativize only absolute paths inside the root', () {
      expect(
        relativePathWithin(
          root: r'\\?\E:\Work\Repo',
          path: r'e:\work\repo\lib\main.dart',
          pathContext: windows,
        ),
        r'lib\main.dart',
      );
      expect(
        relativePathWithin(
          root: '/home/me/repo',
          path: '/home/me/repo',
          pathContext: posix,
        ),
        '.',
      );
      expect(
        relativePathWithin(
          root: '/home/me/repo',
          path: 'lib/main.dart',
          pathContext: posix,
        ),
        isNull,
      );
      expect(
        relativePathWithin(
          root: '/home/me/repo',
          path: '/home/me/other/a.dart',
          pathContext: posix,
        ),
        isNull,
      );
    });

    test('keep POSIX comparisons byte-exact for verbatim-looking names', () {
      expect(
        isSamePath(r'/tmp/\\?\E:', r'/tmp/E:', pathContext: posix),
        isFalse,
      );
    });
  });
}
