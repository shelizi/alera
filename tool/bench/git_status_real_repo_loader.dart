part of 'git_status_real_repo_bench.dart';

int _findNthSpace(String str, int n) {
  var count = 0;
  for (var i = 0; i < str.length; i++) {
    if (str.codeUnitAt(i) == 32) {
      count++;
      if (count == n) return i;
    }
  }
  return -1;
}

GitChangeStatus? _mapStatusChar(String ch) {
  return switch (ch) {
    'M' => GitChangeStatus.modified,
    'A' => GitChangeStatus.added,
    'D' => GitChangeStatus.deleted,
    'R' => GitChangeStatus.renamed,
    'C' => GitChangeStatus.copied,
    'T' => GitChangeStatus.modified,
    'U' => GitChangeStatus.modified,
    '?' => GitChangeStatus.untracked,
    _ => null,
  };
}

class ParsedGitStatus {
  final String repoPath;
  final String porcelainVersion;
  final List<GitChangeEntry> entries;

  const ParsedGitStatus({
    required this.repoPath,
    required this.porcelainVersion,
    required this.entries,
  });
}

Future<ParsedGitStatus> collectRealEntries(String repoPath) async {
  var porcelainVersion = 'porcelain v2';
  var res = await Process.run('git', [
    '-C',
    repoPath,
    'status',
    '--porcelain=v2',
    '-z',
    '--untracked-files=all',
  ], stdoutEncoding: null);

  var isV2 = true;
  if (res.exitCode != 0) {
    porcelainVersion = 'porcelain v1';
    isV2 = false;
    res = await Process.run('git', [
      '-C',
      repoPath,
      'status',
      '--porcelain',
      '-z',
      '--untracked-files=all',
    ], stdoutEncoding: null);
    if (res.exitCode != 0) {
      final err = res.stderr != null
          ? utf8.decode(res.stderr as List<int>, allowMalformed: true)
          : '';
      throw ProcessException(
        'git',
        ['status'],
        'git status failed with exit code ${res.exitCode}: $err',
        res.exitCode,
      );
    }
  }

  final bytes = res.stdout as List<int>;
  final records = <String>[];
  var start = 0;
  for (var i = 0; i < bytes.length; i++) {
    if (bytes[i] == 0) {
      records.add(utf8.decode(bytes.sublist(start, i), allowMalformed: true));
      start = i + 1;
    }
  }
  if (start < bytes.length) {
    final tail = utf8.decode(bytes.sublist(start), allowMalformed: true);
    if (tail.isNotEmpty) records.add(tail);
  }

  final entries = <GitChangeEntry>[];

  if (isV2) {
    for (var i = 0; i < records.length; i++) {
      final record = records[i];
      if (record.isEmpty || record.startsWith('#')) continue;

      if (record.startsWith('? ')) {
        final path = record.substring(2);
        entries.add(
          GitChangeEntry(
            path: path,
            area: GitChangeArea.untracked,
            status: GitChangeStatus.untracked,
          ),
        );
      } else if (record.startsWith('! ')) {
        continue;
      } else if (record.startsWith('1 ')) {
        if (record.length < 4) continue;
        final xChar = record[2];
        final yChar = record[3];
        final pathIdx = _findNthSpace(record, 8);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);

        if (xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(path: path, area: GitChangeArea.staged, status: s),
            );
          }
        }
        if (yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(
                path: path,
                area: GitChangeArea.unstaged,
                status: s,
              ),
            );
          }
        }
      } else if (record.startsWith('2 ')) {
        if (record.length < 4) continue;
        final xChar = record[2];
        final yChar = record[3];
        final pathIdx = _findNthSpace(record, 9);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);
        final origPath = i + 1 < records.length ? records[++i] : null;

        if (xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(
                path: path,
                area: GitChangeArea.staged,
                status: s,
                oldPath: origPath,
              ),
            );
          }
        }
        if (yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(
                path: path,
                area: GitChangeArea.unstaged,
                status: s,
                oldPath: origPath,
              ),
            );
          }
        }
      } else if (record.startsWith('u ')) {
        final pathIdx = _findNthSpace(record, 10);
        if (pathIdx == -1 || pathIdx + 1 >= record.length) continue;
        final path = record.substring(pathIdx + 1);
        entries.add(
          GitChangeEntry(
            path: path,
            area: GitChangeArea.unstaged,
            status: GitChangeStatus.modified,
          ),
        );
      }
    }
  } else {
    // v1 fallback
    for (var i = 0; i < records.length; i++) {
      final record = records[i];
      if (record.isEmpty || record.startsWith('#')) continue;
      if (record.startsWith('?? ')) {
        entries.add(
          GitChangeEntry(
            path: record.substring(3),
            area: GitChangeArea.untracked,
            status: GitChangeStatus.untracked,
          ),
        );
      } else if (record.startsWith('!! ')) {
        continue;
      } else if (record.length >= 4) {
        final xChar = record[0];
        final yChar = record[1];
        final path = record.substring(3);
        final isRename =
            xChar == 'R' || xChar == 'C' || yChar == 'R' || yChar == 'C';
        final origPath = isRename && i + 1 < records.length
            ? records[++i]
            : null;

        if (xChar != ' ' && xChar != '.') {
          final s = _mapStatusChar(xChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(
                path: path,
                area: GitChangeArea.staged,
                status: s,
                oldPath: origPath,
              ),
            );
          }
        }
        if (yChar != ' ' && yChar != '.') {
          final s = _mapStatusChar(yChar);
          if (s != null) {
            entries.add(
              GitChangeEntry(
                path: path,
                area: GitChangeArea.unstaged,
                status: s,
                oldPath: origPath,
              ),
            );
          }
        }
      }
    }
  }

  return ParsedGitStatus(
    repoPath: repoPath,
    porcelainVersion: porcelainVersion,
    entries: entries,
  );
}
