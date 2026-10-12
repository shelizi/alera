import 'package:alera_mobile/src/design_system/icons/alera_icons.dart';
import 'package:alera_mobile/src/features/runtime/domain/workspace_summary.dart';
import 'package:flutter/material.dart';

/// Only offer local branches here. The managed-workspace API's
/// `reuseExistingBranch` flag expects a local branch, not a remote ref.
List<String> findWorkspaceBranches(
  Iterable<String> localBranches,
  String query,
) {
  final needle = query.trim().toLowerCase();
  final matches = <String>{
    for (final branch in localBranches)
      if (branch.trim().isNotEmpty && branch.toLowerCase().contains(needle))
        branch,
  }.toList();
  matches.sort((a, b) => a.toLowerCase().compareTo(b.toLowerCase()));
  return matches;
}

WorkspaceSummary? findExistingBranchWorkspace(
  Iterable<WorkspaceSummary> workspaces,
  String projectId,
  String branch,
) {
  for (final workspace in workspaces) {
    if (workspace.projectId == projectId &&
        workspace.branch == branch &&
        !workspace.isArchived) {
      return workspace;
    }
  }
  return null;
}

Future<String?> chooseWorkspaceBranch(
  BuildContext context, {
  required String projectId,
  required List<String> localBranches,
  required List<WorkspaceSummary> workspaces,
}) async {
  return showSearch<String?>(
    context: context,
    delegate: _WorkspaceBranchSearch(
      projectId: projectId,
      localBranches: localBranches,
      workspaces: workspaces,
    ),
  );
}

class _WorkspaceBranchSearch extends SearchDelegate<String?> {
  _WorkspaceBranchSearch({
    required this.projectId,
    required this.localBranches,
    required this.workspaces,
  }) : super(searchFieldLabel: 'Search local branches');

  final String projectId;
  final List<String> localBranches;
  final List<WorkspaceSummary> workspaces;

  @override
  List<Widget> buildActions(BuildContext context) => <Widget>[
    if (query.isNotEmpty)
      IconButton(
        tooltip: 'Clear search',
        onPressed: () => query = '',
        icon: const Icon(Icons.clear),
      ),
  ];

  @override
  Widget buildLeading(BuildContext context) => IconButton(
    tooltip: 'Back',
    onPressed: () => close(context, null),
    icon: const Icon(Icons.arrow_back),
  );

  @override
  Widget buildResults(BuildContext context) => _results(context);

  @override
  Widget buildSuggestions(BuildContext context) => _results(context);

  Widget _results(BuildContext context) {
    final matches = findWorkspaceBranches(localBranches, query);
    if (matches.isEmpty) {
      return const Center(child: Text('No matching local branches'));
    }
    return ListView.builder(
      itemCount: matches.length,
      itemBuilder: (context, index) {
        final branch = matches[index];
        final existing = findExistingBranchWorkspace(
          workspaces,
          projectId,
          branch,
        );
        return ListTile(
          leading: const Icon(AleraIcons.gitBranch),
          title: Text(branch, maxLines: 1, overflow: TextOverflow.ellipsis),
          subtitle: Text(
            existing == null
                ? 'Open in a new workspace'
                : 'Open existing workspace: ${existing.name}',
          ),
          onTap: () => close(context, branch),
        );
      },
    );
  }
}
