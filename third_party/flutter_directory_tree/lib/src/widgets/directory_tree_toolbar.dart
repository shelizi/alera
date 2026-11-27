// lib/src/widgets/directory_tree_toolbar.dart
import 'dart:async';

import 'package:directory_tree/directory_tree.dart' as core;
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';

/// A utility bar providing filter input and bulk expansion controls.
///
/// **Behavior:**
/// * **Filtering:** Updates [DirectoryTreeController.filterQuery] after a debounce delay (140ms).
/// * **Expand/Collapse All:** Triggers batch updates on [DirectoryTreeController.expansions].
/// * **Status:** Shows the count of currently visible nodes.
class DirectoryTreeToolbar extends StatefulWidget {
  /// Creates a toolbar bound to [controller].
  const DirectoryTreeToolbar({
    super.key,
    required this.controller,
    this.hintText = 'Filter…',
    this.showExpandCollapse = true,
  });

  /// The controller used to filter nodes and manage expansion.
  final DirectoryTreeController controller;

  /// Placeholder text for the filter field.
  final String hintText;

  /// Whether to show the "Expand All" and "Collapse All" buttons.
  final bool showExpandCollapse;

  @override
  State<DirectoryTreeToolbar> createState() => _DirectoryTreeToolbarState();
}

class _DirectoryTreeToolbarState extends State<DirectoryTreeToolbar> {
  late final TextEditingController _text = TextEditingController(
    text: widget.controller.filterQuery,
  );
  Timer? _debounce;
  int _visibleCount = 0;

  void _syncFromController() {
    final want = widget.controller.filterQuery;
    if (want != _text.text) {
      _text.text = want;
      _text.selection = TextSelection.collapsed(offset: _text.text.length);
    }
    setState(() {
      _visibleCount = widget.controller.visibleNodes.length;
    });
  }

  @override
  void initState() {
    super.initState();
    _visibleCount = widget.controller.visibleNodes.length;
    _text.addListener(() {
      final t = _text.text;
      _debounce?.cancel();
      _debounce = Timer(const Duration(milliseconds: 140), () {
        if (widget.controller.filterQuery != t) {
          widget.controller.filterQuery = t;
        }
      });
      setState(() {});
    });
    widget.controller.addListener(_syncFromController);
  }

  @override
  void dispose() {
    _debounce?.cancel();
    widget.controller.removeListener(_syncFromController);
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      color: Theme.of(context).colorScheme.surface,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: _text,
                decoration: InputDecoration(
                  hintText: widget.hintText,
                  prefixIcon: const Icon(Icons.search, size: 18),
                  isDense: true,
                  border: const OutlineInputBorder(),
                  suffixIcon: _text.text.isEmpty
                      ? null
                      : IconButton(
                          icon: const Icon(Icons.clear, size: 18),
                          tooltip: 'Clear filter',
                          onPressed: () => _text.clear(),
                        ),
                ),
              ),
            ),
            if (widget.showExpandCollapse) const SizedBox(width: 8),
            if (widget.showExpandCollapse)
              Tooltip(
                message: 'Expand all',
                child: IconButton(
                  icon: const Icon(Icons.unfold_more),
                  onPressed: () {
                    final folders = widget.controller.data.nodes.values
                        .where((n) =>
                            n.type == core.NodeType.folder ||
                            n.type == core.NodeType.root)
                        .map((n) => n.id);
                    widget.controller.expansions.expandAll(folders);
                  },
                ),
              ),
            if (widget.showExpandCollapse)
              Tooltip(
                message: 'Collapse all',
                child: IconButton(
                  icon: const Icon(Icons.unfold_less),
                  onPressed: () {
                    final rootId = widget.controller.data.visibleRootId;
                    widget.controller.expansions.performBatch(() {
                      widget.controller.expansions.collapseAll();
                      widget.controller.expansions.setExpanded(rootId, true);
                    });
                  },
                ),
              ),
            const SizedBox(width: 4),
            Text(
              '$_visibleCount',
              style: Theme.of(context).textTheme.labelSmall,
            ),
            const SizedBox(width: 4),
          ],
        ),
      ),
    );
  }
}
