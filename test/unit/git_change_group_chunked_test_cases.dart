part of 'git_change_group_test.dart';

void _registerChunkedGroupTests() {
  group('GitChangeGroup chunked', () {
    test(
      'chunked grouping preserves sync output and yields between chunks',
      () async {
        final entries = _randomEntries(180, 23)
          ..add(
            const GitChangeEntry(
              path: 'lib/duplicate.dart',
              area: .staged,
              status: .modified,
              added: 1,
            ),
          )
          ..add(
            const GitChangeEntry(
              path: 'lib/duplicate.dart',
              area: .unstaged,
              status: .modified,
              removed: 2,
            ),
          );
        var yieldCount = 0;

        final chunked = await GitChangeGroup.fromEntriesChunked(
          entries,
          chunkSize: 3,
          onChunk: (_) => yieldCount += 1,
        );
        final expected = GitChangeGroup.fromEntries(entries);

        expect(yieldCount, greaterThan(0));
        _expectGroupsEquivalent(chunked, expected);

        final unifiedChunked = await GitChangeGroup.unifiedFromEntriesChunked(
          entries,
          chunkSize: 3,
        );
        _expectGroupsEquivalent(
          unifiedChunked,
          GitChangeGroup.unifiedFromEntries(entries),
        );
      },
    );

    test(
      'chunked rebind uses native entry indices without source object lookup',
      () async {
        final reboundEntries = <GitChangeEntry>[
          _entry('lib/alpha.dart', area: .unstaged),
          _entry('lib/beta.dart', area: .unstaged),
        ];
        final sourceEntries = reboundEntries.map(_copyEntry).toList();
        final group = GitChangeGroup(
          area: .unstaged,
          entries: <GitChangeEntry>[
            _copyEntry(sourceEntries[1]),
            _copyEntry(sourceEntries[0]),
          ],
          entryIndices: const <int>[1, 0],
          treeRows: <GitChangeTreeRow>[
            GitChangeTreeRow(
              kind: .file,
              name: 'beta.dart',
              path: 'lib/beta.dart',
              depth: 1,
              fileCount: 1,
              entry: _copyEntry(sourceEntries[1]),
              entryIndex: 1,
            ),
            GitChangeTreeRow(
              kind: .file,
              name: 'alpha.dart',
              path: 'lib/alpha.dart',
              depth: 1,
              fileCount: 1,
              entry: _copyEntry(sourceEntries[0]),
              entryIndex: 0,
            ),
          ],
        );

        final rebound = await GitChangeGroup.rebindEntryInstancesChunked(
          <GitChangeGroup>[group],
          sourceEntries: sourceEntries,
          reboundEntries: reboundEntries,
          chunkSize: 1,
        );

        expect(identical(rebound.single.entries[0], reboundEntries[1]), isTrue);
        expect(identical(rebound.single.entries[1], reboundEntries[0]), isTrue);
        expect(
          identical(rebound.single.treeRows[0].entry, reboundEntries[1]),
          isTrue,
        );
        expect(
          identical(rebound.single.treeRows[1].entry, reboundEntries[0]),
          isTrue,
        );
      },
    );

    test(
      'chunked rebind preserves reconciled entry identity in rows',
      () async {
        final previous = <GitChangeEntry>[
          _entry('lib/alpha.dart', area: .unstaged),
          _entry('lib/beta.dart', area: .unstaged),
        ];
        final inserted = _entry('lib/inserted.dart', area: .unstaged);
        final next = <GitChangeEntry>[inserted, ...previous.map(_copyEntry)];
        final reconciled = await reconcileGitChangeEntryInstancesChunked(
          previous,
          next,
          chunkSize: 1,
        );
        final rebound = await GitChangeGroup.rebindEntryInstancesChunked(
          GitChangeGroup.fromEntries(next),
          sourceEntries: next,
          reboundEntries: reconciled,
          chunkSize: 1,
        );

        expect(identical(reconciled[0], inserted), isTrue);
        expect(identical(reconciled[1], previous[0]), isTrue);
        expect(identical(reconciled[2], previous[1]), isTrue);
        final rows = rebound.single.treeRows
            .where((row) => row.entry != null)
            .toList(growable: false);
        expect(rows.map((row) => row.entry!.path).toList(), <String>[
          'lib/alpha.dart',
          'lib/beta.dart',
          'lib/inserted.dart',
        ]);
        expect(identical(rows[0].entry, previous[0]), isTrue);
        expect(identical(rows[1].entry, previous[1]), isTrue);
        expect(identical(rows[2].entry, inserted), isTrue);
      },
    );
  });
}
