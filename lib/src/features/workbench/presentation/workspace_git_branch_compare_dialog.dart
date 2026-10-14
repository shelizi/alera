import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:flutter/material.dart';

class const GitBranchCompareSelection({
  required this.baseRef,
  required this.headRef,
}) {
  final String baseRef;
  final String headRef;
}

/// The user selects refs only; reading or comparing does not check them out.
Future<GitBranchCompareSelection?> showGitBranchCompareDialog({
  required BuildContext context,
  required List<String> branches,
  required String currentBranch,
  String? initialBaseRef,
  String? initialHeadRef,
}) {
  return showDialog<GitBranchCompareSelection>(
    context: context,
    builder: (context) => _BranchCompareDialog(
      branches: branches,
      currentBranch: currentBranch,
      initialBaseRef: initialBaseRef,
      initialHeadRef: initialHeadRef,
    ),
  );
}

class const _BranchCompareDialog({
  required this.branches,
  required this.currentBranch,
  this.initialBaseRef,
  this.initialHeadRef,
}) extends StatefulWidget {
  final List<String> branches;
  final String currentBranch;
  final String? initialBaseRef;
  final String? initialHeadRef;

  @override
  State<_BranchCompareDialog> createState() => _BranchCompareDialogState();
}

class _BranchCompareDialogState extends State<_BranchCompareDialog> {
  late final TextEditingController _base;
  late final TextEditingController _head;

  @override
  void initState() {
    super.initState();
    final branches = widget.branches;
    final base =
        widget.initialBaseRef != null &&
            branches.contains(widget.initialBaseRef)
        ? widget.initialBaseRef!
        : branches.contains('main')
        ? 'main'
        : branches.contains(widget.currentBranch)
        ? widget.currentBranch
        : branches.first;
    final head =
        widget.initialHeadRef != null &&
            branches.contains(widget.initialHeadRef)
        ? widget.initialHeadRef!
        : widget.currentBranch != base &&
              branches.contains(widget.currentBranch)
        ? widget.currentBranch
        : branches.firstWhere((branch) => branch != base, orElse: () => base);
    _base = TextEditingController(text: base)..addListener(_refresh);
    _head = TextEditingController(text: head)..addListener(_refresh);
  }

  void _refresh() {
    if (mounted) setState(() {});
  }

  bool get _canCompare =>
      widget.branches.contains(_base.text) &&
      widget.branches.contains(_head.text) &&
      _base.text != _head.text;

  void _swap() {
    final previous = _base.text;
    _base.text = _head.text;
    _head.text = previous;
  }

  @override
  void dispose() {
    _base.dispose();
    _head.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final entries = widget.branches
        .map(
          (branch) => DropdownMenuEntry<String>(value: branch, label: branch),
        )
        .toList(growable: false);
    final maxWidth = MediaQuery.sizeOf(context).width - AleraTokens.space16 * 4;
    final width = maxWidth.clamp(250.0, 460.0).toDouble();
    return AlertDialog(
      title: const Text('Compare Branches'),
      content: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: width),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            const Text(
              'Compare changes on the second branch since its merge base with the first branch.',
            ),
            const SizedBox(height: AleraTokens.space16),
            DropdownMenu<String>(
              key: const ValueKey<String>('branch-compare-base'),
              controller: _base,
              width: width,
              label: const Text('Base branch'),
              enableFilter: true,
              enableSearch: true,
              requestFocusOnTap: true,
              menuHeight: 260,
              dropdownMenuEntries: entries,
            ),
            Align(
              alignment: Alignment.center,
              child: IconButton(
                tooltip: 'Swap branches',
                onPressed: _swap,
                icon: const Icon(Icons.swap_vert),
              ),
            ),
            DropdownMenu<String>(
              key: const ValueKey<String>('branch-compare-head'),
              controller: _head,
              width: width,
              label: const Text('Compare branch'),
              enableFilter: true,
              enableSearch: true,
              requestFocusOnTap: true,
              menuHeight: 260,
              dropdownMenuEntries: entries,
            ),
            const SizedBox(height: AleraTokens.space12),
            Text(
              'Mode: Merge base to compare branch (A...B)',
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.foregroundMuted),
            ),
            if (!_canCompare)
              Padding(
                padding: const EdgeInsets.only(top: AleraTokens.space8),
                child: Text(
                  'Select two different existing branches.',
                  style: Theme.of(context).textTheme.bodySmall
                      ?.copyWith(color: AleraTokens.warning),
                ),
              ),
          ],
        ),
      ),
      actions: <Widget>[
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _canCompare
              ? () => Navigator.of(context).pop(
                  GitBranchCompareSelection(
                    baseRef: _base.text,
                    headRef: _head.text,
                  ),
                )
              : null,
          child: const Text('Compare'),
        ),
      ],
    );
  }
}
