import 'package:alera/src/features/workbench/application/retired_workspace_invalidation.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workspace_git_branch_compare_cache_provider.g.dart';

@Riverpod(keepAlive: true)
WorkspaceGitBranchCompareCache workspaceGitBranchCompareCache(
  Ref ref,
  String workspaceId,
  String tabId,
) {
  invalidateWhenWorkspaceRetired(ref, workspaceId);
  ref.listen<bool>(
    workbenchControllerProvider.select(
      (state) => state.tabsFor(workspaceId).any((tab) => tab.id == tabId),
    ),
    (previous, next) {
      if (previous == true && !next) ref.invalidateSelf();
    },
  );
  final cache = WorkspaceGitBranchCompareCache();
  ref.onDispose(cache.clear);
  return cache;
}
