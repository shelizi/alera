import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_search_field.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

Future<void> compareWorkspaceFileWithLatestGitRevision({
  required BuildContext context,
  required WidgetRef ref,
  required Workspace workspace,
  required String relativePath,
  WorkspaceSourceControlScope? sourceControlScope,
}) async {
  final target = await _prepareGitFileCompare(
    context: context,
    ref: ref,
    workspace: workspace,
    relativePath: relativePath,
    sourceControlScope: sourceControlScope,
  );
  if (target == null || !context.mounted) return;
  try {
    final compareRef = await target.backend.currentBranch(target.scope.path);
    if (!context.mounted) return;
    await _openGitFileRevisionComparison(
      context: context,
      ref: ref,
      workspace: workspace,
      target: target,
      compareRef: compareRef,
    );
  } on Object catch (error) {
    if (context.mounted) {
      _showGitCompareError(context, error);
    }
  }
}

Future<void> compareWorkspaceFileWithBranch({
  required BuildContext context,
  required WidgetRef ref,
  required Workspace workspace,
  required String relativePath,
  WorkspaceSourceControlScope? sourceControlScope,
}) async {
  final target = await _prepareGitFileCompare(
    context: context,
    ref: ref,
    workspace: workspace,
    relativePath: relativePath,
    sourceControlScope: sourceControlScope,
  );
  if (target == null || !context.mounted) return;
  try {
    final results = await Future.wait<Object>(<Future<Object>>[
      target.backend.listBranches(target.scope.path),
      target.backend.currentBranch(target.scope.path),
    ]);
    if (!context.mounted) return;
    final branches = results[0] as List<String>;
    final currentBranch = results[1] as String;
    if (branches.isEmpty) {
      AleraToast.show(
        context,
        message: context.tr('No Git branches are available to compare.'),
        tone: .error,
      );
      return;
    }
    final compareRef = await showDialog<String>(
      context: context,
      builder: (context) => _GitBranchPickerDialog(
        branches: branches,
        currentBranch: currentBranch,
      ),
    );
    if (compareRef == null || !context.mounted) return;
    await _openGitFileRevisionComparison(
      context: context,
      ref: ref,
      workspace: workspace,
      target: target,
      compareRef: compareRef,
    );
  } on Object catch (error) {
    if (context.mounted) {
      _showGitCompareError(context, error);
    }
  }
}

Future<_GitFileCompareTarget?> _prepareGitFileCompare({
  required BuildContext context,
  required WidgetRef ref,
  required Workspace workspace,
  required String relativePath,
  required WorkspaceSourceControlScope? sourceControlScope,
}) async {
  final backend = ref.read(gitBackendProvider);
  final normalizedRelativePath = normalizeWorkspaceRelativePath(relativePath);
  if (normalizedRelativePath == null) return null;

  Object? lastError;
  try {
    final absoluteFilePath = p.normalize(
      p.joinAll(<String>[
        p.absolute(workspace.path),
        ...normalizedRelativePath.split('/'),
      ]),
    );
    final repositoryRoot = await backend.repositoryRoot(
      p.dirname(absoluteFilePath),
    );
    final discoveredScope = repositoryRoot == null
        ? null
        : _scopeForRepositoryRoot(
            workspace: workspace,
            repositoryRoot: repositoryRoot,
          );
    final candidateScopes = <WorkspaceSourceControlScope>[
      if (discoveredScope != null) discoveredScope,
      if (sourceControlScope != null &&
          !_sameSourceControlScope(sourceControlScope, discoveredScope))
        sourceControlScope,
      if (discoveredScope == null || !discoveredScope.isWorkspaceRoot)
        WorkspaceSourceControlScope(
          workspaceId: workspace.id,
          workspacePath: workspace.path,
          path: workspace.path,
        ),
    ];
    for (final scope in candidateScopes) {
      final sourceFilePath = scope.toSourceRelativePath(relativePath);
      if (sourceFilePath == null || sourceFilePath.isEmpty) continue;
      if (!await backend.isGitRepository(scope.path)) continue;
      return _GitFileCompareTarget(
        backend: backend,
        scope: scope,
        workspaceRelativePath: relativePath,
        sourceFilePath: sourceFilePath,
      );
    }
  } on Object catch (error) {
    lastError = error;
  }

  if (context.mounted) {
    if (lastError != null) {
      _showGitCompareError(context, lastError);
    } else {
      AleraToast.show(
        context,
        message: context.tr('Git comparison is unavailable for this file.'),
        tone: .error,
      );
    }
  }
  return null;
}

WorkspaceSourceControlScope _scopeForRepositoryRoot({
  required Workspace workspace,
  required String repositoryRoot,
}) {
  final workspacePath = p.normalize(p.absolute(workspace.path));
  final rootPath = p.normalize(p.absolute(repositoryRoot));
  if (p.equals(rootPath, workspacePath) ||
      p.isWithin(rootPath, workspacePath)) {
    return WorkspaceSourceControlScope(
      workspaceId: workspace.id,
      workspacePath: workspace.path,
      path: workspace.path,
    );
  }
  if (p.isWithin(workspacePath, rootPath)) {
    final relativeRoot = normalizeSourceControlRootRelativePath(
      p.relative(rootPath, from: workspacePath).replaceAll('\\', '/'),
    );
    if (relativeRoot != null) {
      return WorkspaceSourceControlScope(
        workspaceId: workspace.id,
        workspacePath: workspace.path,
        path: rootPath,
        relativeRoot: relativeRoot,
      );
    }
  }
  return WorkspaceSourceControlScope(
    workspaceId: workspace.id,
    workspacePath: workspace.path,
    path: workspace.path,
  );
}

bool _sameSourceControlScope(
  WorkspaceSourceControlScope left,
  WorkspaceSourceControlScope? right,
) {
  if (right == null) return false;
  return p.equals(p.normalize(left.path), p.normalize(right.path)) &&
      left.relativeRoot == right.relativeRoot;
}

Future<void> _openGitFileRevisionComparison({
  required BuildContext context,
  required WidgetRef ref,
  required Workspace workspace,
  required _GitFileCompareTarget target,
  required String compareRef,
}) async {
  final revision = await target.backend.latestFileRevision(
    path: target.scope.path,
    filePath: target.sourceFilePath,
    gitRef: compareRef,
  );
  if (!context.mounted) return;
  if (revision == null) {
    AleraToast.show(
      context,
      message: context.tr('This file has no revision on $compareRef.'),
      tone: .error,
    );
    return;
  }
  await ref
      .read(workbenchControllerProvider.notifier)
      .openGitFileRevisionDiffTab(
        workspace: workspace,
        relativePath: target.workspaceRelativePath,
        gitDiffRoot: target.scope.relativeRoot,
        revisionOid: revision.oid,
        compareRef: compareRef,
        subject: revision.subject,
        preview: false,
      );
}

void _showGitCompareError(BuildContext context, Object error) {
  AleraToast.show(
    context,
    message: context.tr('Could not compare file with Git: $error'),
    tone: .error,
  );
}

final class _GitFileCompareTarget {
  const _GitFileCompareTarget({
    required this.backend,
    required this.scope,
    required this.workspaceRelativePath,
    required this.sourceFilePath,
  });

  final GitBackend backend;
  final WorkspaceSourceControlScope scope;
  final String workspaceRelativePath;
  final String sourceFilePath;
}

class const _GitBranchPickerDialog({
  required this.branches,
  required this.currentBranch,
}) extends StatefulWidget {
  final List<String> branches;
  final String currentBranch;

  @override
  State<_GitBranchPickerDialog> createState() => _GitBranchPickerDialogState();
}

class _GitBranchPickerDialogState extends State<_GitBranchPickerDialog> {
  final TextEditingController _searchController = TextEditingController();
  final FocusNode _searchFocusNode = FocusNode(
    debugLabel: 'git-branch-compare',
  );
  String _query = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _searchFocusNode.requestFocus();
      }
    });
  }

  @override
  void dispose() {
    _searchController.dispose();
    _searchFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = _query.trim().toLowerCase();
    final branches = widget.branches
        .where(
          (branch) => query.isEmpty || branch.toLowerCase().contains(query),
        )
        .toList(growable: false);
    return AleraDialog(
      maxWidth: AleraTokens.dialogCompactWidth,
      maxHeight: AleraTokens.dialogMaxHeight,
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space16),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: <Widget>[
            Text(
              context.tr('Compare With Branch'),
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: AleraTokens.space12),
            AleraSearchField(
              key: const ValueKey<String>('git-compare-branch-search'),
              controller: _searchController,
              focusNode: _searchFocusNode,
              hintText: context.tr('Search Branches'),
              dense: true,
              onChanged: (value) => setState(() => _query = value),
            ),
            const SizedBox(height: AleraTokens.space8),
            Flexible(
              child: branches.isEmpty
                  ? Center(
                      child: Text(
                        context.tr('No matching branches.'),
                        style: Theme.of(context).textTheme.bodyMedium
                            ?.copyWith(color: AleraTokens.foregroundMuted),
                      ),
                    )
                  : ListView.builder(
                      shrinkWrap: true,
                      itemCount: branches.length,
                      itemBuilder: (context, index) {
                        final branch = branches[index];
                        final current = branch == widget.currentBranch;
                        return ListTile(
                          key: ValueKey<String>('git-compare-branch:$branch'),
                          dense: true,
                          leading: const Icon(AleraIcons.gitBranch, size: 16),
                          title: Text(
                            branch,
                            maxLines: 1,
                            overflow: .ellipsis,
                            style: AleraTokens.monoStyle.copyWith(
                              color: AleraTokens.foreground,
                            ),
                          ),
                          trailing: current
                              ? Text(
                                  context.tr('Current'),
                                  style: Theme.of(context).textTheme.labelSmall
                                      ?.copyWith(
                                        color: AleraTokens.foregroundMuted,
                                      ),
                                )
                              : null,
                          onTap: () => Navigator.of(context).pop(branch),
                        );
                      },
                    ),
            ),
            const SizedBox(height: AleraTokens.space8),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: Text(context.tr('Cancel')),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
