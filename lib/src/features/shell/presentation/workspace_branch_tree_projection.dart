part of 'alera_shell_page.dart';

/// Keep the local branch picker independent of remote metadata failures.
/// A current local checkout must remain visible even if discovery is stale.
@visibleForTesting
List<String> filterWorkspaceLocalBranches({
  required List<String> branches,
  required String currentBranch,
  required List<String> remoteNames,
}) {
  final prefixes = <String>[
    for (final name in remoteNames)
      if (name.trim().isNotEmpty) '${name.trim()}/',
  ];
  return (<String>{
    for (final branch in branches)
      if (!prefixes.any(branch.startsWith)) branch,
    if (currentBranch != 'HEAD' && currentBranch.trim().isNotEmpty)
      currentBranch,
  }.toList()..sort());
}

@visibleForTesting
List<WorkspaceBranchTreeRow> buildWorkspaceBranchTreeRows(
  List<String> branches,
) {
  final roots = <String, _WorkspaceBranchTreeNode>{};
  for (final raw in branches) {
    final branch = raw.trim();
    if (branch.isEmpty) continue;
    final segments = branch
        .split('/')
        .where((segment) => segment.isNotEmpty)
        .toList(growable: false);
    var nodes = roots;
    var path = '';
    _WorkspaceBranchTreeNode? node;
    for (final segment in segments) {
      path = path.isEmpty ? segment : '$path/$segment';
      node = nodes.putIfAbsent(
        segment,
        () => _WorkspaceBranchTreeNode(segment: segment, fullPath: path),
      );
      nodes = node.children;
    }
    node?.branch = branch;
  }

  final rows = <WorkspaceBranchTreeRow>[];
  void append(Map<String, _WorkspaceBranchTreeNode> nodes, int depth) {
    final sorted = nodes.values.toList()
      ..sort((a, b) => a.segment.compareTo(b.segment));
    for (final node in sorted) {
      rows.add(
        WorkspaceBranchTreeRow(
          segment: node.segment,
          fullPath: node.fullPath,
          branch: node.branch,
          depth: depth,
          hasChildren: node.children.isNotEmpty,
        ),
      );
      append(node.children, depth + 1);
    }
  }

  append(roots, 0);
  return rows;
}

@visibleForTesting
class const WorkspaceBranchTreeRow({
  required this.segment,
  required this.fullPath,
  required this.branch,
  required this.depth,
  required this.hasChildren,
}) {
  final String segment;
  final String fullPath;
  final String? branch;
  final int depth;
  final bool hasChildren;
}

final class _WorkspaceBranchTreeNode {
  _WorkspaceBranchTreeNode({required this.segment, required this.fullPath});

  final String segment;
  final String fullPath;
  final Map<String, _WorkspaceBranchTreeNode> children =
      <String, _WorkspaceBranchTreeNode>{};
  String? branch;
}

String? _normalizedWorkspaceBranch(String? value) {
  final branch = value?.trim();
  return branch == null || branch.isEmpty || branch == 'HEAD' ? null : branch;
}
