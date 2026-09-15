part of 'workspace_git_diff_panel.dart';

// Above this entry count the unified regroup runs chunked off the frame;
// the previous groups (or an empty list on first entry) stay visible
// until the projection lands.
const int _unifiedSyncEntryLimit = 4000;

extension on _WorkspaceGitDiffPanelState {
  GitStatusResult _filteredStatus(GitStatusResult status) {
    final query = _filterController.text.trim().toLowerCase();
    if (identical(_cachedStatusSource, status) && _cachedFilterQuery == query) {
      return _cachedFilteredStatus;
    }
    var filtered = status;
    if (query.isNotEmpty) {
      final entries = status.entries
          .where((entry) {
            return entry.path.toLowerCase().contains(query) ||
                (entry.oldPath?.toLowerCase().contains(query) ?? false);
          })
          .toList(growable: false);
      filtered = GitStatusResult(
        entries: entries,
        groups: GitChangeGroup.fromEntries(entries),
      );
    }
    _cachedStatusSource = status;
    _cachedFilterQuery = query;
    _cachedFilteredStatus = filtered;
    return filtered;
  }

  List<GitChangeGroup> _groupsFor(GitStatusResult status) {
    if (identical(_cachedGroupsSource, status) &&
        _cachedGroupsMode == widget.groupMode) {
      return _cachedGroups;
    }
    if (widget.groupMode == GitDiffGroupMode.unified &&
        status.entries.length > _unifiedSyncEntryLimit) {
      if (!identical(_pendingUnifiedSource, status)) {
        _pendingUnifiedSource = status;
        _scheduleUnifiedGroups(status);
      }
      return _cachedGroups;
    }
    _pendingUnifiedSource = null;
    _unifiedGroupsGeneration += 1;
    _cachedGroupsSource = status;
    _cachedGroupsMode = widget.groupMode;
    _cachedGroups = widget.groupMode == GitDiffGroupMode.unified
        ? _unifiedGroupsFor(status)
        : status.effectiveGroups;
    return _cachedGroups;
  }

  List<GitChangeGroup> _unifiedGroupsFor(GitStatusResult status) {
    final groups = status.groups;
    return groups.isNotEmpty
        ? GitChangeGroup.unifiedFromGroups(groups)
        : GitChangeGroup.unifiedFromEntries(status.entries);
  }

  Future<List<GitChangeGroup>> _unifiedGroupsForChunked(
    GitStatusResult status,
  ) {
    final groups = status.groups;
    return groups.isNotEmpty
        ? GitChangeGroup.unifiedFromGroupsChunked(groups)
        : GitChangeGroup.unifiedFromEntriesChunked(status.entries);
  }

  void _scheduleUnifiedGroups(GitStatusResult status) {
    final generation = ++_unifiedGroupsGeneration;
    _unifiedGroupsForChunked(status).then((groups) {
      if (!mounted || generation != _unifiedGroupsGeneration) {
        debugPrint(
          '[stale-completion] request_type=workspace_git_diff_unified_groups generation=$generation current_generation=$_unifiedGroupsGeneration mounted=$mounted',
        );
        return;
      }
      _applyUnifiedGroups(status, groups);
    });
  }

  Set<String> _visibleCollapsibleKeys(WorkspaceSourceControlState? state) {
    if (state == null) {
      return const <String>{};
    }
    final groups = _groupsFor(_filteredStatus(state.status));
    if (identical(_cachedCollapsibleGroups, groups)) {
      return _cachedCollapsibleKeys;
    }
    final keys = <String>{};
    for (final group in groups) {
      if (group.entries.isEmpty) {
        continue;
      }
      keys.add(_sectionKeyForGroup(group));
      final folderAreaKey = group.unified ? 'unified' : group.area.key;
      for (final row in group.treeRows) {
        if (row.kind == GitChangeTreeRowKind.directory) {
          keys.add('folder:$folderAreaKey:${row.path}');
        }
      }
    }
    _cachedCollapsibleGroups = groups;
    _cachedCollapsibleKeys = keys;
    return keys;
  }
}
