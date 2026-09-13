import 'dart:math';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('GitChangeGroup', () {
    test('fromEntries keeps staged unstaged and untracked sections', () {
      const entries = <GitChangeEntry>[
        GitChangeEntry(
          path: 'lib/new.dart',
          area: .untracked,
          status: .untracked,
        ),
        GitChangeEntry(
          path: 'lib/dirty.dart',
          area: .unstaged,
          status: .modified,
        ),
        GitChangeEntry(
          path: 'lib/staged.dart',
          area: .staged,
          status: .modified,
        ),
      ];

      final groups = GitChangeGroup.fromEntries(entries);
      expect(groups.map((group) => group.label).toList(), <String>[
        'Staged',
        'Unstaged',
        'Untracked',
      ]);
      expect(groups.every((group) => !group.unified), isTrue);
    });

    test('fromEntries omits empty areas and sorts paths once', () {
      final entries = <GitChangeEntry>[
        _entry('z.txt'),
        _entry('a/b.txt'),
        _entry('a/c/d.txt'),
      ];

      final groups = GitChangeGroup.fromEntries(entries);
      expect(groups, hasLength(1));
      expect(groups.single.area, GitChangeArea.untracked);
      expect(
        groups.single.entries.map((entry) => entry.path).toList(),
        <String>['a/b.txt', 'a/c/d.txt', 'z.txt'],
      );
    });

    test('fromEntries and unifiedFromEntries return empty for no entries', () {
      expect(GitChangeGroup.fromEntries(const []), isEmpty);
      expect(GitChangeGroup.unifiedFromEntries(const []), isEmpty);
    });

    test('unifiedFromEntries merges areas and keeps dual-area paths', () {
      const entries = <GitChangeEntry>[
        GitChangeEntry(
          path: 'lib/new.dart',
          area: .untracked,
          status: .untracked,
        ),
        GitChangeEntry(
          path: 'lib/dirty.dart',
          area: .unstaged,
          status: .modified,
        ),
        GitChangeEntry(
          path: 'lib/dirty.dart',
          area: .staged,
          status: .modified,
        ),
        GitChangeEntry(
          path: 'lib/staged.dart',
          area: .staged,
          status: .modified,
        ),
      ];

      final groups = GitChangeGroup.unifiedFromEntries(entries);
      expect(groups, hasLength(1));
      final group = groups.single;
      expect(group.unified, isTrue);
      expect(group.label, 'Changes');
      expect(
        group.entries
            .map((entry) => '${entry.area.key}:${entry.path}')
            .toList(),
        <String>[
          'staged:lib/dirty.dart',
          'unstaged:lib/dirty.dart',
          'untracked:lib/new.dart',
          'staged:lib/staged.dart',
        ],
      );
      expect(
        group.treeRows
            .where((row) => row.kind == GitChangeTreeRowKind.file)
            .map((row) => '${row.entry!.area.key}:${row.path}')
            .toList(),
        <String>[
          'staged:lib/dirty.dart',
          'unstaged:lib/dirty.dart',
          'untracked:lib/new.dart',
          'staged:lib/staged.dart',
        ],
      );
    });

    test('tree rows put directories before files and keep nested counts', () {
      final groups = GitChangeGroup.fromEntries(<GitChangeEntry>[
        _entry('z.txt'),
        _entry('a.txt'),
        _entry('lib/src/nested/c.dart'),
        _entry('lib/src/b.dart'),
        _entry('lib/a.dart'),
      ]);

      expect(_rowLabels(groups.single.treeRows), <String>[
        'dir:0:lib:3',
        'dir:1:lib/src:2',
        'dir:2:lib/src/nested:1',
        'file:3:lib/src/nested/c.dart:1',
        'file:2:lib/src/b.dart:1',
        'file:1:lib/a.dart:1',
        'file:0:a.txt:1',
        'file:0:z.txt:1',
      ]);
    });

    test('tree rows keep depth and counts for deep untracked paths', () {
      final entries = <GitChangeEntry>[
        _entry('pkg/src/a/b/c/d/e/file.txt'),
        _entry('pkg/src/a/b/other.txt'),
        _entry('pkg/src/a/sibling.txt'),
        _entry('pkg/top.txt'),
      ];

      final rows = GitChangeGroup.fromEntries(entries).single.treeRows;
      expect(_rowLabels(rows), <String>[
        'dir:0:pkg:4',
        'dir:1:pkg/src:3',
        'dir:2:pkg/src/a:3',
        'dir:3:pkg/src/a/b:2',
        'dir:4:pkg/src/a/b/c:1',
        'dir:5:pkg/src/a/b/c/d:1',
        'dir:6:pkg/src/a/b/c/d/e:1',
        'file:7:pkg/src/a/b/c/d/e/file.txt:1',
        'file:4:pkg/src/a/b/other.txt:1',
        'file:3:pkg/src/a/sibling.txt:1',
        'file:1:pkg/top.txt:1',
      ]);
    });

    test('fromEntries builds independent trees per mixed area', () {
      final entries = <GitChangeEntry>[
        _entry('shared/new.txt'),
        _entry('shared/dirty.txt', area: .unstaged),
        _entry('only-staged.txt', area: .staged),
      ];

      final groups = GitChangeGroup.fromEntries(entries);
      expect(groups.map((group) => group.area).toList(), <GitChangeArea>[
        GitChangeArea.staged,
        GitChangeArea.unstaged,
        GitChangeArea.untracked,
      ]);
      expect(_rowLabels(groups[0].treeRows), <String>[
        'file:0:only-staged.txt:1',
      ]);
      expect(_rowLabels(groups[1].treeRows), <String>[
        'dir:0:shared:1',
        'file:1:shared/dirty.txt:1',
      ]);
      expect(_rowLabels(groups[2].treeRows), <String>[
        'dir:0:shared:1',
        'file:1:shared/new.txt:1',
      ]);
    });

    test(
      'tree rows match a split-based reference for mixed and deep paths',
      () {
        final mixed = _randomEntries(120, 11)
          ..add(_entry('lib/dirty.dart', area: .staged))
          ..add(_entry('lib/dirty.dart', area: .unstaged));
        _expectRowsMatch(
          GitChangeGroup.fromEntries(mixed),
          mixed,
          unified: false,
        );
        _expectRowsMatch(
          GitChangeGroup.unifiedFromEntries(mixed),
          mixed,
          unified: true,
        );

        final deep = _deepUntrackedEntries(400);
        _expectRowsMatch(
          GitChangeGroup.fromEntries(deep),
          deep,
          unified: false,
        );
      },
    );

    test('7.5k deep untracked tree keeps every file and directory counts', () {
      final entries = _deepUntrackedEntries(7475);
      final groups = GitChangeGroup.fromEntries(entries);
      expect(groups, hasLength(1));
      final group = groups.single;
      expect(group.entries, hasLength(7475));

      final fileRows = group.treeRows
          .where((row) => row.kind == GitChangeTreeRowKind.file)
          .toList(growable: false);
      expect(fileRows, hasLength(7475));
      expect(
        fileRows.map((row) => row.path).toSet(),
        entries.map((entry) => entry.path).toSet(),
      );

      final rootDirs = group.treeRows.where(
        (row) => row.kind == GitChangeTreeRowKind.directory && row.depth == 0,
      );
      expect(rootDirs.fold<int>(0, (sum, row) => sum + row.fileCount), 7475);
    });
  });
}

GitChangeEntry _entry(
  String path, {
  GitChangeArea area = GitChangeArea.untracked,
}) {
  return GitChangeEntry(
    path: path,
    area: area,
    status: area == GitChangeArea.untracked ? .untracked : .modified,
  );
}

List<String> _rowLabels(List<GitChangeTreeRow> rows) {
  return <String>[
    for (final row in rows)
      '${row.kind == GitChangeTreeRowKind.directory ? 'dir' : 'file'}:'
          '${row.depth}:${row.path}:${row.fileCount}',
  ];
}

void _expectRowsMatch(
  List<GitChangeGroup> groups,
  List<GitChangeEntry> entries, {
  required bool unified,
}) {
  if (unified) {
    expect(groups, hasLength(entries.isEmpty ? 0 : 1));
    if (groups.isEmpty) {
      return;
    }
    expect(
      _rowLabels(groups.single.treeRows),
      _rowLabels(_referenceTreeRows(_sortedUnified(entries))),
    );
    return;
  }

  final byArea = <GitChangeArea, List<GitChangeEntry>>{
    GitChangeArea.staged: <GitChangeEntry>[],
    GitChangeArea.unstaged: <GitChangeEntry>[],
    GitChangeArea.untracked: <GitChangeEntry>[],
  };
  for (final entry in entries) {
    byArea[entry.area]!.add(entry);
  }
  const areaOrder = <GitChangeArea>[
    GitChangeArea.staged,
    GitChangeArea.unstaged,
    GitChangeArea.untracked,
  ];
  expect(groups.map((group) => group.area).toList(), <GitChangeArea>[
    for (final area in areaOrder)
      if (byArea[area]!.isNotEmpty) area,
  ]);
  for (final group in groups) {
    final sorted = List<GitChangeEntry>.of(byArea[group.area]!)
      ..sort((a, b) => a.path.compareTo(b.path));
    expect(
      group.entries.map((entry) => entry.path).toList(),
      sorted.map((entry) => entry.path).toList(),
    );
    expect(_rowLabels(group.treeRows), _rowLabels(_referenceTreeRows(sorted)));
  }
}

List<GitChangeEntry> _sortedUnified(List<GitChangeEntry> entries) {
  int areaIndex(GitChangeArea area) {
    return switch (area) {
      GitChangeArea.staged => 0,
      GitChangeArea.unstaged => 1,
      GitChangeArea.untracked => 2,
    };
  }

  return List<GitChangeEntry>.of(entries)..sort((a, b) {
    final byPath = a.path.compareTo(b.path);
    if (byPath != 0) {
      return byPath;
    }
    return areaIndex(a.area).compareTo(areaIndex(b.area));
  });
}

List<GitChangeEntry> _randomEntries(int count, int seed) {
  final rng = Random(seed);
  const dirs = <String>['lib', 'src', 'test', 'tool', 'rust', 'docs', 'assets'];
  const names = <String>['a', 'b', 'c', 'widget', 'service', 'model', 'foo'];
  final areas = GitChangeArea.values;
  return <GitChangeEntry>[
    for (var i = 0; i < count; i++)
      _entry(
        [
          dirs[rng.nextInt(dirs.length)],
          for (var depth = 0; depth < rng.nextInt(6); depth++)
            names[rng.nextInt(names.length)],
          'file_$i.dart',
        ].join('/'),
        area: areas[rng.nextInt(areas.length)],
      ),
  ];
}

List<GitChangeEntry> _deepUntrackedEntries(int count) {
  const roots = <String>[
    '.tmp-alera-devin-live-e2e/target/debug',
    'alera-ewdk-verify/payload/data',
    'alera-ewdk-verify/out/obj',
  ];
  const mid = <String>['fingerprint', 'incremental', 'deps', 'icons', 'assets'];
  return <GitChangeEntry>[
    for (var i = 0; i < count; i++)
      _entry(
        '${roots[i % roots.length]}/${mid[i % mid.length]}/n$i/deep/file_$i.o',
      ),
  ];
}

List<GitChangeTreeRow> _referenceTreeRows(List<GitChangeEntry> entries) {
  final root = _ReferenceDir(name: '', path: '', depth: 0);
  for (final entry in entries) {
    final parts = entry.path
        .split('/')
        .where((part) => part.isNotEmpty)
        .toList(growable: false);
    if (parts.isEmpty) {
      continue;
    }
    var node = root;
    for (var index = 0; index < parts.length - 1; index++) {
      node = node.child(parts[index]);
    }
    node.files.add(entry);
  }
  final rows = <GitChangeTreeRow>[];
  root.emitChildren(rows);
  return rows;
}

class _ReferenceDir {
  _ReferenceDir({required this.name, required this.path, required this.depth});

  final String name;
  final String path;
  final int depth;
  final Map<String, _ReferenceDir> dirs = <String, _ReferenceDir>{};
  final List<GitChangeEntry> files = <GitChangeEntry>[];

  _ReferenceDir child(String childName) {
    return dirs.putIfAbsent(childName, () {
      final childPath = path.isEmpty ? childName : '$path/$childName';
      return _ReferenceDir(
        name: childName,
        path: childPath,
        depth: path.isEmpty ? 0 : depth + 1,
      );
    });
  }

  int get fileCount {
    var count = files.length;
    for (final dir in dirs.values) {
      count += dir.fileCount;
    }
    return count;
  }

  void emitChildren(List<GitChangeTreeRow> rows) {
    final dirList = dirs.values.toList(growable: false)
      ..sort((a, b) => a.name.compareTo(b.name));
    for (final dir in dirList) {
      dir.emit(rows);
    }
    for (final entry in files) {
      final lastSlash = entry.path.lastIndexOf('/');
      rows.add(
        GitChangeTreeRow(
          kind: GitChangeTreeRowKind.file,
          name: lastSlash == -1
              ? entry.path
              : entry.path.substring(lastSlash + 1),
          path: entry.path,
          depth: path.isEmpty ? 0 : depth + 1,
          fileCount: 1,
          entry: entry,
        ),
      );
    }
  }

  void emit(List<GitChangeTreeRow> rows) {
    rows.add(
      GitChangeTreeRow(
        kind: GitChangeTreeRowKind.directory,
        name: name,
        path: path,
        depth: depth,
        fileCount: fileCount,
        entry: null,
      ),
    );
    emitChildren(rows);
  }
}
