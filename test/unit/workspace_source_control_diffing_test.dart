import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';
import 'fake_source_control_watcher.dart';

const _workspacePath = '/tmp/workspace';

GitStatusResult _statusWith(List<GitChangeEntry> entries) {
  return GitStatusResult(
    entries: entries,
    groups: GitChangeGroup.fromEntries(entries),
  );
}

Future<(ProviderContainer, WorkspaceSourceControlController)> _boot(
  FakeGitBackend backend,
  FakeSourceControlWatcher watcher,
) async {
  final container = ProviderContainer(
    overrides: [
      gitBackendProvider.overrideWithValue(backend),
      sourceControlWatcherProvider.overrideWithValue(watcher),
    ],
  );
  final provider = workspaceSourceControlControllerProvider(_workspacePath);
  final subscription = container.listen(provider, (_, _) {});
  addTearDown(subscription.close);
  addTearDown(container.dispose);
  await container.read(provider.future);
  await Future.pause(const Duration(milliseconds: 10));
  return (container, container.read(provider.notifier));
}

Future<void> _reloadFromWatcher(FakeSourceControlWatcher watcher) async {
  watcher.emitChange();
  await Future.pause(const Duration(milliseconds: 350));
}

List<GitChangeEntry> _initialEntries() => <GitChangeEntry>[
  GitChangeEntry(
    path: 'lib/alpha.dart',
    area: .unstaged,
    status: .modified,
    added: 2,
    removed: 1,
  ),
  GitChangeEntry(
    path: 'lib/beta.dart',
    area: .unstaged,
    status: .modified,
    added: 3,
    removed: 0,
  ),
  GitChangeEntry(
    path: 'lib/gamma.dart',
    area: .unstaged,
    status: .added,
    added: 8,
    removed: 0,
  ),
];

void main() {
  test('entry reconciliation preserves shifted entries by full value', () {
    final previous = _initialEntries();
    final inserted = GitChangeEntry(
      path: 'lib/inserted.dart',
      area: .unstaged,
      status: .modified,
      added: 1,
      removed: 0,
    );
    final merged = reconcileGitChangeEntryInstances(previous, <GitChangeEntry>[
      inserted,
      ..._initialEntries(),
    ]);

    expect(identical(merged[0], inserted), isTrue);
    expect(identical(merged[1], previous[0]), isTrue);
    expect(identical(merged[2], previous[1]), isTrue);
    expect(identical(merged[3], previous[2]), isTrue);
  });

  test(
    'identical watcher reload preserves the complete state instance',
    () async {
      final backend = FakeGitBackend()
        ..gitStatusResult = _statusWith(_initialEntries())
        ..gitRepositoryStateResult = GitRepositoryState(
          branch: 'main',
          upstream: 'origin/main',
          ahead: 1,
          behind: 2,
          hasConflicts: false,
          headMessage: 'latest',
        )
        ..gitStashEntries = <GitStashEntry>[
          GitStashEntry(
            index: 0,
            reference: 'stash@{0}',
            message: 'work in progress',
            oid: 'stash-oid',
          ),
        ];
      final watcher = FakeSourceControlWatcher();
      addTearDown(watcher.dispose);
      final (container, _) = await _boot(backend, watcher);
      final provider = workspaceSourceControlControllerProvider(_workspacePath);
      final before = container.read(provider).requireValue;

      backend.gitStatusResult = _statusWith(_initialEntries());
      backend.gitRepositoryStateResult = GitRepositoryState(
        branch: 'main',
        upstream: 'origin/main',
        ahead: 1,
        behind: 2,
        hasConflicts: false,
        headMessage: 'latest',
      );
      backend.gitStashEntries = <GitStashEntry>[
        GitStashEntry(
          index: 0,
          reference: 'stash@{0}',
          message: 'work in progress',
          oid: 'stash-oid',
        ),
      ];
      await _reloadFromWatcher(watcher);

      final after = container.read(provider).requireValue;
      expect(identical(after, before), isTrue);
      expect(identical(after.status, before.status), isTrue);
      for (var index = 0; index < before.status.entries.length; index += 1) {
        expect(
          identical(after.status.entries[index], before.status.entries[index]),
          isTrue,
        );
      }
    },
  );

  test(
    'a changed entry gets a new instance while others are preserved',
    () async {
      final backend = FakeGitBackend()
        ..gitStatusResult = _statusWith(_initialEntries());
      final watcher = FakeSourceControlWatcher();
      addTearDown(watcher.dispose);
      final (container, _) = await _boot(backend, watcher);
      final provider = workspaceSourceControlControllerProvider(_workspacePath);
      final before = container.read(provider).requireValue;

      final changedEntries = _initialEntries();
      changedEntries[1] = GitChangeEntry(
        path: 'lib/beta.dart',
        area: .unstaged,
        status: .modified,
        added: 30,
        removed: 0,
      );
      backend.gitStatusResult = _statusWith(changedEntries);
      await _reloadFromWatcher(watcher);

      final after = container.read(provider).requireValue;
      expect(identical(after, before), isFalse);
      expect(identical(after.status, before.status), isFalse);
      expect(
        identical(after.status.entries[0], before.status.entries[0]),
        isTrue,
      );
      expect(
        identical(after.status.entries[1], before.status.entries[1]),
        isFalse,
      );
      expect(
        identical(after.status.entries[2], before.status.entries[2]),
        isTrue,
      );
      expect(
        identical(
          after.status.groups.single.entries[0],
          before.status.groups.single.entries[0],
        ),
        isTrue,
      );
      expect(
        identical(
          after.status.groups.single.entries[2],
          before.status.groups.single.entries[2],
        ),
        isTrue,
      );
    },
  );

  test(
    'completely different content does not preserve any entry instance',
    () async {
      final backend = FakeGitBackend()
        ..gitStatusResult = _statusWith(_initialEntries().take(2).toList());
      final watcher = FakeSourceControlWatcher();
      addTearDown(watcher.dispose);
      final (container, _) = await _boot(backend, watcher);
      final provider = workspaceSourceControlControllerProvider(_workspacePath);
      final before = container.read(provider).requireValue;

      backend.gitStatusResult = _statusWith(<GitChangeEntry>[
        GitChangeEntry(
          path: 'test/delta_test.dart',
          area: .staged,
          status: .renamed,
          oldPath: 'test/old_delta_test.dart',
          added: 10,
          removed: 4,
        ),
        GitChangeEntry(
          path: 'test/epsilon_test.dart',
          area: .untracked,
          status: .untracked,
        ),
      ]);
      await _reloadFromWatcher(watcher);

      final after = container.read(provider).requireValue;
      expect(identical(after, before), isFalse);
      expect(identical(after.status, before.status), isFalse);
      for (var index = 0; index < after.status.entries.length; index += 1) {
        expect(
          identical(after.status.entries[index], before.status.entries[index]),
          isFalse,
        );
      }
    },
  );

  test(
    'refresh preserves status and entry instances when content is unchanged',
    () async {
      final backend = FakeGitBackend()
        ..gitStatusResult = _statusWith(_initialEntries());
      final watcher = FakeSourceControlWatcher();
      addTearDown(watcher.dispose);
      final (container, controller) = await _boot(backend, watcher);
      final provider = workspaceSourceControlControllerProvider(_workspacePath);
      final before = container.read(provider).requireValue;

      await controller.refresh();

      final after = container.read(provider).requireValue;
      expect(identical(after.status, before.status), isTrue);
      for (var index = 0; index < before.status.entries.length; index += 1) {
        expect(
          identical(after.status.entries[index], before.status.entries[index]),
          isTrue,
        );
      }
    },
  );
}
