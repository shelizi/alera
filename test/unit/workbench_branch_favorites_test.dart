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
}
