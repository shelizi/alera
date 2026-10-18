part of 'workspace_git_diff_surface.dart';

/// Owns scrolling for the mixed file-header / editable-pane row list.
/// AutoScrollTag can reveal a lazy row even when another file precedes it.
class _EditableSideBySideDiffList extends StatefulWidget {
  const _EditableSideBySideDiffList({
    super.key,
    required this.navigation,
    required this.rows,
    required this.contentWidth,
  });

  final _DiffChangeNavigator navigation;
  final _DiffRows rows;
  final double contentWidth;

  @override
  State<_EditableSideBySideDiffList> createState() =>
      _EditableSideBySideDiffListState();
}

class _EditableSideBySideDiffListState
    extends State<_EditableSideBySideDiffList> {
  late final AutoScrollController _vertical;
  late final ScrollController _horizontal;

  @override
  void initState() {
    super.initState();
    _vertical = AutoScrollController(
      suggestedRowHeight: 24,
      keepScrollOffset: false,
    );
    _horizontal = ScrollController(keepScrollOffset: false);
    widget.navigation.addListener(_navigate);
  }

  @override
  void didUpdateWidget(covariant _EditableSideBySideDiffList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.navigation != widget.navigation) {
      oldWidget.navigation.removeListener(_navigate);
      widget.navigation.addListener(_navigate);
      _navigate();
    }
  }

  void _navigate() {
    final target = widget.navigation.target;
    if (target == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_vertical.hasClients) return;
      unawaited(
        _vertical.scrollToIndex(
          target.row,
          preferPosition: AutoScrollPosition.begin,
          duration: const Duration(milliseconds: 80),
        ),
      );
    });
  }

  @override
  void dispose() {
    widget.navigation.removeListener(_navigate);
    _vertical.dispose();
    _horizontal.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final list = ListView.builder(
          controller: _vertical,
          padding: const EdgeInsets.only(
            bottom: workspaceEditorBottomScrollPadding,
          ),
          itemCount: widget.rows.length,
          itemBuilder: (context, index) => AutoScrollTag(
            key: ValueKey<String>('git-diff-editable-row-$index'),
            index: index,
            controller: _vertical,
            child: widget.rows.rowAt(index).build(context),
          ),
        );
        return SingleChildScrollView(
          controller: _horizontal,
          scrollDirection: Axis.horizontal,
          physics: const _NoImplicitHorizontalScrollPhysics(),
          child: SizedBox(
            width: math.max(widget.contentWidth, constraints.maxWidth),
            height: constraints.maxHeight,
            child: list,
          ),
        );
      },
    );
  }
}
