import 'dart:convert';

import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';

// Project identity shares favorites across linked worktrees; the relative root
// keeps nested repositories separate from the project's main repository.
String branchFavoritesScopeKey(String projectId, String? relativeRoot) =>
    jsonEncode(<String?>[
      projectId,
      normalizeSourceControlRootRelativePath(relativeRoot),
    ]);

WorkbenchViewPrefs toggleWorkbenchBranchFavorite({
  required WorkbenchViewPrefs prefs,
  required String scopeKey,
  required String branch,
}) {
  final favorites = <String>{...?prefs.favoriteBranchesByRepository[scopeKey]};
  if (!favorites.remove(branch)) favorites.add(branch);
  final repositories = <String, Set<String>>{
    ...prefs.favoriteBranchesByRepository,
  };
  if (favorites.isEmpty) {
    repositories.remove(scopeKey);
  } else {
    repositories[scopeKey] = Set.unmodifiable(favorites);
  }
  return prefs.copyWith(
    favoriteBranchesByRepository: Map<String, Set<String>>.unmodifiableOf(
      repositories,
    ),
  );
}

List<String> favoriteBranchesFirst(
  Iterable<String> branches,
  Set<String> favorites,
) => branches.toList()
  ..sort((a, b) {
    final favoriteOrder =
        (favorites.contains(b) ? 1 : 0) - (favorites.contains(a) ? 1 : 0);
    return favoriteOrder != 0 ? favoriteOrder : a.compareTo(b);
  });
