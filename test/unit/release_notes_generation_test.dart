import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import '../../tool/release/generate_release_notes.dart' as release_notes;

void main() {
  group('history parsing', () {
    test('splits sha and subject and detects pull request merges', () {
      final entries = release_notes.parseHistory(
        'aaa1 Merge pull request #138 from leynier/chore/launcher-icons\n'
        'bbb2 fix: patch something directly on main\n'
        '\n',
      );
      expect(entries, hasLength(2));
      expect(entries[0].sha, 'aaa1');
      expect(entries[0].pullRequestNumber, 138);
      expect(entries[1].subject, 'fix: patch something directly on main');
      expect(entries[1].pullRequestNumber, isNull);
    });

    test('identifies release bookkeeping commits for both products', () {
      expect(
        release_notes.isReleaseBookkeepingSubject('release: v0.18.0'),
        isTrue,
      );
      expect(
        release_notes.isReleaseBookkeepingSubject('release: mobile-v0.0.1'),
        isTrue,
      );
      expect(
        release_notes.isReleaseBookkeepingSubject(
          'release: v1.2.3 v0.4.0-mobile',
        ),
        isTrue,
      );
      expect(
        release_notes.isReleaseBookkeepingSubject('fix: release notes tool'),
        isFalse,
      );
    });
  });

  group('scope predicates', () {
    test('mobile scope only matches entries touching mobile paths', () {
      expect(
        release_notes.pathsMatchScope('mobile', ['mobile/pubspec.yaml']),
        isTrue,
      );
      expect(
        release_notes.pathsMatchScope('mobile', [
          'lib/main.dart',
          'mobile/lib/main.dart',
        ]),
        isTrue,
      );
      expect(
        release_notes.pathsMatchScope('mobile', [
          'lib/main.dart',
          'landing/index.html',
        ]),
        isFalse,
      );
    });

    test('desktop scope ignores mobile-only and landing-only entries', () {
      expect(
        release_notes.pathsMatchScope('desktop', ['lib/main.dart']),
        isTrue,
      );
      expect(
        release_notes.pathsMatchScope('desktop', [
          'mobile/lib/main.dart',
          'rust/src/api/git.rs',
        ]),
        isTrue,
      );
      expect(
        release_notes.pathsMatchScope('desktop', [
          'mobile/lib/main.dart',
          'landing/index.html',
        ]),
        isFalse,
      );
    });
  });

  group('notes composition', () {
    test('mixes pull request and commit bullets with a compare link', () {
      final notes = release_notes.composeReleaseNotes(
        repo: 'leynier/alera',
        tag: 'mobile-v0.0.2',
        previousTag: 'mobile-v0.0.1',
        bullets: [
          '* feat: add mobile app by @leynier in '
              'https://github.com/leynier/alera/pull/130',
          '* fix: hotfix on main by @leynier in '
              'https://github.com/leynier/alera/commit/abc123',
        ],
      );
      expect(notes, contains("## What's Changed"));
      expect(notes, contains('pull/130'));
      expect(notes, contains('commit/abc123'));
      expect(
        notes,
        contains(
          '**Full Changelog**: https://github.com/leynier/alera/compare/'
          'mobile-v0.0.1...mobile-v0.0.2',
        ),
      );
    });

    test('falls back to a commits link when there is no previous tag', () {
      final notes = release_notes.composeReleaseNotes(
        repo: 'leynier/alera',
        tag: 'mobile-v0.0.1',
        bullets: ['* feat: add mobile app'],
      );
      expect(
        notes,
        contains(
          '**Full Changelog**: '
          'https://github.com/leynier/alera/commits/mobile-v0.0.1',
        ),
      );
    });

    test('keeps the changelog link when no entries match the scope', () {
      final notes = release_notes.composeReleaseNotes(
        repo: 'leynier/alera',
        tag: 'v0.19.0',
        previousTag: 'v0.18.0',
        bullets: const [],
      );
      expect(notes, isNot(contains("## What's Changed")));
      expect(notes, contains('compare/v0.18.0...v0.19.0'));
    });
  });

  group('release note history bullets', () {
    final missingPull = ProcessException(
      'gh',
      ['api', 'repos/shelizi/alera/pulls/91'],
      'gh: Not Found (HTTP 404)',
      1,
    );
    final forbiddenPull = ProcessException(
      'gh',
      ['api', 'repos/shelizi/alera/pulls/91'],
      'gh: Forbidden (HTTP 403)',
      1,
    );
    const mergeEntry = release_notes.HistoryEntry(
      sha: 'abc123',
      subject: 'Merge pull request #91 from leynier/some-feature',
    );
    const commitMetadata =
        '{"author":{"login":"shelizi"},"commit":{"author":{"name":"Neo"}}}';

    test(
      'falls back to commit URL when a fork lacks a historical PR',
      () async {
        final requested = <String>[];
        final seen = <int>{};
        final bullet = await release_notes.resolveHistoryBullet(
          'shelizi/alera',
          mergeEntry,
          seen,
          fetchGh: (args) async {
            final endpoint = args.last;
            requested.add(endpoint);
            if (endpoint.endsWith('/pulls/91')) throw missingPull;
            if (endpoint.endsWith('/commits/abc123')) return commitMetadata;
            throw StateError('Unexpected API request: $endpoint');
          },
        );
        expect(
          bullet,
          contains('https://github.com/shelizi/alera/commit/abc123'),
        );
        expect(bullet, contains('by @shelizi'));
        expect(
          seen,
          isEmpty,
          reason: 'Missing fork PRs must not be marked seen',
        );
        expect(requested, [
          'repos/shelizi/alera/pulls/91',
          'repos/shelizi/alera/commits/abc123',
        ]);
      },
    );

    test('deduplicates resolved PRs with valid fork links', () async {
      final seen = <int>{};
      Future<String> api(List<String> args) async =>
          '{"title":"Fix drag selection","user":{"login":"contributor"}}';
      final first = await release_notes.resolveHistoryBullet(
        'shelizi/alera',
        mergeEntry,
        seen,
        fetchGh: api,
      );
      final duplicate = await release_notes.resolveHistoryBullet(
        'shelizi/alera',
        mergeEntry,
        seen,
        fetchGh: api,
      );
      expect(first, contains('Fix drag selection by @contributor'));
      expect(first, contains('https://github.com/shelizi/alera/pull/91'));
      expect(duplicate, isNull);
      expect(seen, {91});
    });

    test(
      'does not suppress permission failures when looking up a PR',
      () async {
        await expectLater(
          release_notes.resolveHistoryBullet(
            'shelizi/alera',
            mergeEntry,
            <int>{},
            fetchGh: (_) async => throw forbiddenPull,
          ),
          throwsA(isA<ProcessException>()),
        );
      },
    );

    test('falls back when commit-to-PR association is missing', () async {
      final requested = <String>[];
      final bullet = await release_notes.resolveHistoryBullet(
        'shelizi/alera',
        const release_notes.HistoryEntry(
          sha: 'def456',
          subject: 'fix: minor feature',
        ),
        <int>{},
        fetchGh: (args) async {
          final endpoint = args.last;
          requested.add(endpoint);
          if (endpoint.endsWith('/pulls')) throw missingPull;
          if (endpoint.endsWith('/commits/def456')) return commitMetadata;
          throw StateError('Unexpected API request: $endpoint');
        },
      );
      expect(
        bullet,
        contains('https://github.com/shelizi/alera/commit/def456'),
      );
      expect(requested, [
        'repos/shelizi/alera/commits/def456/pulls',
        'repos/shelizi/alera/commits/def456',
      ]);
    });
  });
  group('option parsing', () {
    test('requires a valid scope and mandatory flags', () {
      expect(
        () => release_notes.parseOptions(['--scope', 'web']),
        throwsArgumentError,
      );
      final options = release_notes.parseOptions([
        '--scope',
        'mobile',
        '--repo',
        'leynier/alera',
        '--tag',
        'mobile-v0.0.2',
        '--target',
        'abc123',
        '--previous-tag',
        'mobile-v0.0.1',
        '--output',
        'notes.md',
      ]);
      expect(options.scope, 'mobile');
      expect(options.previousTag, 'mobile-v0.0.1');
      expect(options.output, 'notes.md');
    });
  });
}
