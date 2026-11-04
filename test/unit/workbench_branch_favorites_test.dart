import 'package:alera/src/features/workbench/application/workbench_branch_favorites.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'favorites persist, remain scoped, and toggle without mutating old prefs',
    () {
      final key = branchFavoritesScopeKey('project', null);
      final nested = branchFavoritesScopeKey('project', 'packages/app');
      final initial = WorkbenchViewPrefs.defaults;
      final favorite = toggleWorkbenchBranchFavorite(
        prefs: initial,
        scopeKey: key,
        branch: 'feature/auth',
      );
      final other = toggleWorkbenchBranchFavorite(
        prefs: favorite,
        scopeKey: nested,
        branch: 'main',
      );
      final restored = WorkbenchViewPrefs.fromJson(other.toMap());
      expect(restored.favoriteBranchesByRepository[key], {'feature/auth'});
      expect(restored.favoriteBranchesByRepository[nested], {'main'});
      expect(initial.favoriteBranchesByRepository, isEmpty);
      expect(
        toggleWorkbenchBranchFavorite(
          prefs: restored,
          scopeKey: key,
          branch: 'feature/auth',
        ).favoriteBranchesByRepository,
        {
          nested: {'main'},
        },
      );
      final legacy = other.toMap()..remove('favoriteBranchesByRepository');
      expect(
        WorkbenchViewPrefs.fromJson(legacy).favoriteBranchesByRepository,
        isEmpty,
      );
    },
  );

  test('favorites are ordered first while all groups stay alphabetical', () {
    expect(favoriteBranchesFirst(['z', 'b', 'a', 'x'], {'z', 'x'}), [
      'x',
      'z',
      'a',
      'b',
    ]);
    expect(
      branchFavoritesScopeKey('p', null),
      branchFavoritesScopeKey('p', '.'),
    );
    expect(
      branchFavoritesScopeKey('p', 'app'),
      isNot(branchFavoritesScopeKey('other', 'app')),
    );
  });

  test('rename preserves favorite and does not change other repositories', () {
    final root = branchFavoritesScopeKey('p', null);
    final nested = branchFavoritesScopeKey('p', 'nested');
    final before = toggleWorkbenchBranchFavorite(
      prefs: toggleWorkbenchBranchFavorite(
        prefs: WorkbenchViewPrefs.defaults,
        scopeKey: root,
        branch: 'feature/old',
      ),
      scopeKey: nested,
      branch: 'feature/old',
    );
    final after = renameWorkbenchBranchFavorite(
      prefs: before,
      scopeKey: root,
      oldBranch: 'feature/old',
      newBranch: 'feature/new',
    );
    expect(after.favoriteBranchesByRepository[root], {'feature/new'});
    expect(after.favoriteBranchesByRepository[nested], {'feature/old'});
    expect(before.favoriteBranchesByRepository[root], {'feature/old'});
    expect(
      renameWorkbenchBranchFavorite(
        prefs: after,
        scopeKey: root,
        oldBranch: 'not-pinned',
        newBranch: 'elsewhere',
      ),
      same(after),
    );
  });
}
