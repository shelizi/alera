import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('buildGitHistoryViewModels', () {
    test('adds incoming and outgoing boundary rows for divergent branches', () {
      const currentRef = GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: 'local',
      );
      const remoteRef = GitHistoryItemRef(
        id: 'refs/remotes/origin/main',
        name: 'origin/main',
        revision: 'remote',
      );
      const result = GitHistoryResult(
        currentRef: currentRef,
        remoteRef: remoteRef,
        mergeBase: 'base',
        hasIncomingChanges: true,
        hasOutgoingChanges: true,
        hasMore: false,
        limit: 50,
        items: <GitHistoryItem>[
          GitHistoryItem(
            id: 'local',
            parentIds: <String>['base'],
            subject: 'Local Commit',
            message: 'Local Commit',
            displayId: 'local',
            references: <GitHistoryItemRef>[currentRef],
          ),
          GitHistoryItem(
            id: 'remote',
            parentIds: <String>['base'],
            subject: 'Remote Commit',
            message: 'Remote Commit',
            displayId: 'remote',
            references: <GitHistoryItemRef>[remoteRef],
          ),
          GitHistoryItem(
            id: 'base',
            parentIds: <String>[],
            subject: 'Base Commit',
            message: 'Base Commit',
            displayId: 'base',
          ),
        ],
      );

      final viewModels = buildGitHistoryViewModels(result);
      final ids = viewModels
          .map((viewModel) => viewModel.historyItem.id)
          .toList(growable: false);

      expect(ids, contains(gitHistoryOutgoingChangesId));
      expect(ids, contains(gitHistoryIncomingChangesId));
      expect(
        ids.indexOf(gitHistoryOutgoingChangesId),
        lessThan(ids.indexOf('local')),
      );
      expect(
        ids.indexOf(gitHistoryIncomingChangesId),
        lessThan(ids.indexOf('base')),
      );
    });

    test('colors current and remote refs before rendering rows', () {
      const currentRef = GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: 'head',
      );
      const remoteRef = GitHistoryItemRef(
        id: 'refs/remotes/origin/main',
        name: 'origin/main',
        revision: 'head',
      );
      const result = GitHistoryResult(
        currentRef: currentRef,
        remoteRef: remoteRef,
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 50,
        items: <GitHistoryItem>[
          GitHistoryItem(
            id: 'head',
            parentIds: <String>[],
            subject: 'Head Commit',
            message: 'Head Commit',
            references: <GitHistoryItemRef>[remoteRef, currentRef],
          ),
        ],
      );

      final references = buildGitHistoryViewModels(result)
          .single
          .historyItem
          .references;

      expect(references.first.id, currentRef.id);
      expect(references.first.color, GitHistoryGraphColorId.ref);
      expect(references.last.id, remoteRef.id);
      expect(references.last.color, GitHistoryGraphColorId.remoteRef);
    });

    test('adds outgoing boundary when head revision is filtered out', () {
      const currentRef = GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: 'head',
      );
      const remoteRef = GitHistoryItemRef(
        id: 'refs/remotes/origin/main',
        name: 'origin/main',
        revision: 'base',
      );
      const result = GitHistoryResult(
        currentRef: currentRef,
        remoteRef: remoteRef,
        mergeBase: 'base',
        hasIncomingChanges: false,
        hasOutgoingChanges: true,
        hasMore: false,
        limit: 50,
        items: <GitHistoryItem>[
          GitHistoryItem(
            id: 'scoped-local',
            parentIds: <String>['base'],
            subject: 'Scoped Local Commit',
            message: 'Scoped Local Commit',
            displayId: 'scoped',
          ),
          GitHistoryItem(
            id: 'base',
            parentIds: <String>[],
            subject: 'Base Commit',
            message: 'Base Commit',
            displayId: 'base',
            references: <GitHistoryItemRef>[remoteRef],
          ),
        ],
      );

      final viewModels = buildGitHistoryViewModels(result);
      final ids = viewModels
          .map((viewModel) => viewModel.historyItem.id)
          .toList(growable: false);
      final outgoing = viewModels.singleWhere(
        (viewModel) => viewModel.historyItem.id == gitHistoryOutgoingChangesId,
      );

      expect(ids.indexOf(gitHistoryOutgoingChangesId), 0);
      expect(ids.indexOf('scoped-local'), 1);
      expect(outgoing.historyItem.parentIds, <String>['scoped-local']);
      expect(
        viewModels[1].inputSwimlanes,
        contains(
          isA<GitHistoryGraphNode>()
              .having((node) => node.id, 'id', 'scoped-local')
              .having(
                (node) => node.color,
                'color',
                GitHistoryGraphColorId.ref,
              ),
        ),
      );
    });

    test('initialSwimlanes seed the first row of a paged continuation', () {
      const firstPage = <GitHistoryItem>[
        GitHistoryItem(
          id: 'c2',
          parentIds: <String>['c1'],
          subject: 'Second Commit',
          message: 'Second Commit',
        ),
      ];
      const secondPage = <GitHistoryItem>[
        GitHistoryItem(
          id: 'c1',
          parentIds: <String>['c0'],
          subject: 'First Commit',
          message: 'First Commit',
        ),
        GitHistoryItem(
          id: 'c0',
          parentIds: <String>[],
          subject: 'Root Commit',
          message: 'Root Commit',
        ),
      ];

      final firstPageViewModels = buildGitHistoryViewModelsFromItems(firstPage);
      final continued = buildGitHistoryViewModelsFromItems(
        secondPage,
        initialSwimlanes: firstPageViewModels.last.outputSwimlanes,
      );
      final fresh = buildGitHistoryViewModelsFromItems(secondPage);

      // The continued page's first row consumes the lane left open by the
      // previous page instead of starting from an empty lane set.
      expect(
        continued.first.inputSwimlanes.map(
          (node) => (id: node.id, color: node.color),
        ),
        firstPageViewModels.last.outputSwimlanes.map(
          (node) => (id: node.id, color: node.color),
        ),
      );
      expect(gitHistoryItemLaneIndex(continued.first), 0);
      // The parent commit stays on the same lane the previous page opened.
      expect(continued.first.outputSwimlanes.single.id, 'c0');
      expect(
        continued.first.outputSwimlanes.single.color,
        firstPageViewModels.last.outputSwimlanes.single.color,
      );
      // Without the seed the same items still render, starting on new lanes.
      expect(fresh.first.inputSwimlanes, isEmpty);
    });

    test('initialSwimlanes only affect the first row, not later rows', () {
      const items = <GitHistoryItem>[
        GitHistoryItem(
          id: 'b',
          parentIds: <String>['a'],
          subject: 'B',
          message: 'B',
        ),
        GitHistoryItem(
          id: 'a',
          parentIds: <String>[],
          subject: 'A',
          message: 'A',
        ),
      ];

      final viewModels = buildGitHistoryViewModelsFromItems(
        items,
        initialSwimlanes: const <GitHistoryGraphNode>[
          GitHistoryGraphNode(id: 'b', color: GitHistoryGraphColorId.lane3),
        ],
      );

      expect(
        viewModels.first.inputSwimlanes.single.color,
        GitHistoryGraphColorId.lane3,
      );
      expect(
        viewModels.last.inputSwimlanes.map(
          (node) => (id: node.id, color: node.color),
        ),
        viewModels.first.outputSwimlanes.map(
          (node) => (id: node.id, color: node.color),
        ),
      );
    });
  });
}
