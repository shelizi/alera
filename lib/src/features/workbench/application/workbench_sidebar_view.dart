import 'package:alera/src/features/workbench/application/workbench_archive_sweep.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_listing.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workspace_agent_run_groups.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Only changes when the visible row order or archive expansion changes.
/// Agent content updates are handled by individual keyed row subscribers.
class WorkbenchSidebarLayout {
  WorkbenchSidebarLayout(List<String> keys, Set<String> collapsedArchivedKeys)
    : keys = List<String>.unmodifiable(keys),
      collapsedArchivedKeys = Set<String>.unmodifiable(collapsedArchivedKeys),
      indexByKey = Map<String, int>.unmodifiable({
        for (var i = 0; i < keys.length; i++) keys[i]: i,
      });

  final List<String> keys;
  final Set<String> collapsedArchivedKeys;
  final Map<String, int> indexByKey;

  bool previousIsWorkspace(int index) =>
      index > 0 && keys[index - 1].startsWith('workspace:');
}

class WorkbenchSidebarView {
  const WorkbenchSidebarView({required this.layout, required this.rowsByKey});

  final WorkbenchSidebarLayout layout;
  final Map<String, WorkbenchSidebarRow> rowsByKey;
}

/// Reconciles the rendered rows by stable key. A background tool event
/// should not rebuild unrelated workspaces, the project headers with
/// unchanged counts, or the list delegate when its visible order is unchanged.
class WorkbenchSidebarViewCache {
  WorkbenchSidebarView? _previous;

  WorkbenchSidebarView project(
    List<WorkbenchSidebarRow> rows, {
    required Set<String> expandedArchivedKeys,
    required String searchQuery,
  }) {
    final collapsedKeys = archivedSidebarCollapseKeysForQuery(
      defaultCollapsedArchivedSidebarKeys(rows, expandedArchivedKeys),
      searchQuery,
    );
    final visible = collapseArchivedSidebarRows(rows, collapsedKeys);
    final previous = _previous;
    final keys = <String>[];
    final byKey = <String, WorkbenchSidebarRow>{};
    for (final row in visible) {
      final key = row.key;
      keys.add(key);
      final old = previous?.rowsByKey[key];
      byKey[key] = old != null && _sameRow(old, row) ? old : row;
    }
    final oldLayout = previous?.layout;
    final layout =
        oldLayout != null &&
            listEquals(oldLayout.keys, keys) &&
            setEquals(oldLayout.collapsedArchivedKeys, collapsedKeys)
        ? oldLayout
        : WorkbenchSidebarLayout(keys, collapsedKeys);
    if (previous != null &&
        identical(layout, oldLayout) &&
        mapEquals(previous.rowsByKey, byKey)) {
      return previous;
    }
    return _previous = WorkbenchSidebarView(
      layout: layout,
      rowsByKey: Map<String, WorkbenchSidebarRow>.unmodifiable(byKey),
    );
  }

  // Include every row field consumed by the renderer or action callbacks.
  // Identity for domain models is conservative: newly published models are
  // forwarded even when their values compare equal.
  bool _sameRow(WorkbenchSidebarRow old, WorkbenchSidebarRow next) {
    if (identical(old, next)) return true;
    return switch ((old, next)) {
      (WorkbenchWorkspaceRow a, WorkbenchWorkspaceRow b) =>
        identical(a.project, b.project) &&
            identical(a.workspace, b.workspace) &&
            a.showProjectChip == b.showProjectChip &&
            a.indent == b.indent &&
            a.expanded == b.expanded &&
            a.hasTerminalTabs == b.hasTerminalTabs &&
            a.visibleChildCount == b.visibleChildCount &&
            a.childrenCollapsed == b.childrenCollapsed &&
            a.isPinnedCopy == b.isPinnedCopy &&
            listEquals(a.agentRuns, b.agentRuns) &&
            _sameGroups(a.agentRunGroups, b.agentRunGroups),
      (WorkbenchProjectHeaderRow a, WorkbenchProjectHeaderRow b) =>
        identical(a.project, b.project) &&
            a.workspaceCount == b.workspaceCount &&
            a.collapsed == b.collapsed &&
            mapEquals(a.agentCounts, b.agentCounts),
      (WorkbenchSectionHeaderRow a, WorkbenchSectionHeaderRow b) =>
        identical(a.section, b.section) &&
            a.workspaceCount == b.workspaceCount &&
            a.collapsed == b.collapsed,
      (WorkbenchArchivedHeaderRow a, WorkbenchArchivedHeaderRow b) =>
        a.scopeId == b.scopeId &&
            a.workspaceCount == b.workspaceCount &&
            a.indent == b.indent,
      (WorkbenchPinnedHeaderRow a, WorkbenchPinnedHeaderRow b) =>
        a.workspaceCount == b.workspaceCount && a.collapsed == b.collapsed,
      (WorkbenchAllHeaderRow a, WorkbenchAllHeaderRow b) =>
        a.workspaceCount == b.workspaceCount && a.collapsed == b.collapsed,
      _ => false,
    };
  }

  bool _sameGroups(
    List<WorkspaceAgentRunGroup> a,
    List<WorkspaceAgentRunGroup> b,
  ) {
    if (identical(a, b)) return true;
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i].kind != b[i].kind || !listEquals(a[i].runs, b[i].runs)) {
        return false;
      }
    }
    return true;
  }
}

// UI-local cache: release row references when the expanded sidebar unmounts.
final _sidebarViewCacheProvider =
    Provider.autoDispose<WorkbenchSidebarViewCache>(
      (ref) => WorkbenchSidebarViewCache(),
    );

final workbenchSidebarViewProvider = Provider.autoDispose<WorkbenchSidebarView>(
  (ref) => ref
      .watch(_sidebarViewCacheProvider)
      .project(
        ref.watch(workbenchSidebarRowsProvider),
        expandedArchivedKeys: ref.watch(
          workbenchArchivedSectionsCollapseProvider,
        ),
        searchQuery: ref.watch(
          workbenchControllerProvider.select((state) => state.searchQuery),
        ),
      ),
);
