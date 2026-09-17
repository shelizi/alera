import 'package:alera/src/features/workbench/application/workspace_search_projection.dart';
import 'package:alera/src/rust/api/workspace_search.dart' as native;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('flat projection preserves native file order and match rows', () {
    final projection = WorkspaceSearchProjection(_result());

    final rows = projection.rows(
      collapsedResultNodeKeys: const <String>{},
      viewAsTree: false,
    );

    expect(rows, hasLength(6));
    expect(
      rows.whereType<WorkspaceSearchProjectionFileRow>().map(
        (row) => row.file.relativePath,
      ),
      <String>['z.dart', 'src/b.dart'],
    );
  });

  test('tree projection sorts directories and files once', () {
    final projection = WorkspaceSearchProjection(_result());

    final rows = projection.rows(
      collapsedResultNodeKeys: const <String>{},
      viewAsTree: true,
    );

    expect(rows.first, isA<WorkspaceSearchProjectionDirectoryRow>());
    expect((rows.first as WorkspaceSearchProjectionDirectoryRow).path, 'src');
    expect(
      rows.whereType<WorkspaceSearchProjectionFileRow>().map(
        (row) => row.file.relativePath,
      ),
      <String>['src/b.dart', 'z.dart'],
    );
  });

  test('collapsed directory skips its descendants', () {
    final projection = WorkspaceSearchProjection(_result());

    final rows = projection.rows(
      collapsedResultNodeKeys: <String>{workspaceSearchDirectoryNodeKey('src')},
      viewAsTree: true,
    );

    expect(rows.whereType<WorkspaceSearchProjectionMatchRow>(), hasLength(2));
    expect(
      rows.whereType<WorkspaceSearchProjectionFileRow>().map(
        (row) => row.file.relativePath,
      ),
      <String>['z.dart'],
    );
  });

  test('collapsible node keys are stable cached instances per view mode', () {
    final projection = WorkspaceSearchProjection(_result());

    final flatA = projection.collapsibleNodeKeys(viewAsTree: false);
    final flatB = projection.collapsibleNodeKeys(viewAsTree: false);
    final treeA = projection.collapsibleNodeKeys(viewAsTree: true);
    final treeB = projection.collapsibleNodeKeys(viewAsTree: true);

    expect(identical(flatA, flatB), isTrue);
    expect(identical(treeA, treeB), isTrue);
    expect(treeA, contains(workspaceSearchDirectoryNodeKey('src')));
    expect(treeA, contains(workspaceSearchFileNodeKey('src/b.dart')));
  });
}

native.WorkspaceSearchResult _result() {
  return const native.WorkspaceSearchResult(
    totalMatches: 4,
    truncated: false,
    files: <native.WorkspaceSearchFileResult>[
      native.WorkspaceSearchFileResult(
        relativePath: 'z.dart',
        contentToken: 'z',
        matches: <native.WorkspaceSearchMatch>[
          native.WorkspaceSearchMatch(
            id: 'z:1',
            line: 1,
            column: 1,
            matchLength: 1,
            lineContent: 'z',
          ),
          native.WorkspaceSearchMatch(
            id: 'z:2',
            line: 2,
            column: 1,
            matchLength: 1,
            lineContent: 'z',
          ),
        ],
      ),
      native.WorkspaceSearchFileResult(
        relativePath: 'src/b.dart',
        contentToken: 'b',
        matches: <native.WorkspaceSearchMatch>[
          native.WorkspaceSearchMatch(
            id: 'b:1',
            line: 1,
            column: 1,
            matchLength: 1,
            lineContent: 'b',
          ),
          native.WorkspaceSearchMatch(
            id: 'b:2',
            line: 2,
            column: 1,
            matchLength: 1,
            lineContent: 'b',
          ),
        ],
      ),
    ],
  );
}
