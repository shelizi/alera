import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';
import 'package:flutter/material.dart';

/// Swimlane rendering for one commit row, shared between the source-control
/// history panel and the main-area commit graph tab.
class const GitHistoryGraph({
  super.key,
  required final GitHistoryItemViewModel viewModel,
}) extends StatelessWidget {
  static double widthFor(GitHistoryItemViewModel viewModel) {
    return GitHistoryGraphPainter.laneWidth *
        ([
              viewModel.inputSwimlanes.length,
              viewModel.outputSwimlanes.length,
              1,
            ].reduce((a, b) => a > b ? a : b) +
            1);
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: widthFor(viewModel),
      height: GitHistoryGraphPainter.laneHeight,
      child: CustomPaint(painter: GitHistoryGraphPainter(viewModel)),
    );
  }
}

class const GitHistoryGraphPainter(final GitHistoryItemViewModel viewModel)
    extends CustomPainter {
  // Match the 28px history row height so vertical lane segments touch across
  // adjacent rows instead of looking dashed. A wider lane pitch also keeps
  // multi-branch graphs readable when several lanes run in parallel.
  static const double laneHeight = 28;
  static const double laneWidth = 16;
  static const double nodeY = laneHeight / 2;
  static const double circleRadius = 4;

  @override
  void paint(Canvas canvas, Size size) {
    final item = viewModel.historyItem;
    final input = viewModel.inputSwimlanes;
    final output = viewModel.outputSwimlanes;
    final inputIndex = input.indexWhere((node) => node.id == item.id);
    final circleIndex = gitHistoryItemLaneIndex(viewModel);
    final circleColor = circleIndex < output.length
        ? output[circleIndex].color
        : circleIndex < input.length
        ? input[circleIndex].color
        : gitHistoryRefColor;
    var outputIndex = 0;

    for (var index = 0; index < input.length; index += 1) {
      final color = input[index].color;
      if (input[index].id == item.id) {
        if (index != circleIndex) {
          _drawPath(
            canvas,
            color,
            Path()
              ..moveTo(laneWidth * (index + 1), 0)
              ..quadraticBezierTo(
                laneWidth * index,
                nodeY,
                laneWidth * (circleIndex + 1),
                nodeY,
              ),
          );
        } else {
          outputIndex += 1;
        }
        continue;
      }
      if (outputIndex < output.length &&
          input[index].id == output[outputIndex].id) {
        final path = Path()..moveTo(laneWidth * (index + 1), 0);
        if (index == outputIndex) {
          path.lineTo(laneWidth * (index + 1), laneHeight);
        } else {
          path
            ..lineTo(laneWidth * (index + 1), 6)
            ..quadraticBezierTo(
              laneWidth * (index + 1),
              nodeY,
              laneWidth * (outputIndex + 1),
              nodeY,
            )
            ..lineTo(laneWidth * (outputIndex + 1), laneHeight);
        }
        _drawPath(canvas, color, path);
        outputIndex += 1;
      }
    }

    for (var index = 1; index < item.parentIds.length; index += 1) {
      final parentIndex = gitHistoryMergeParentLaneIndex(
        viewModel,
        item.parentIds[index],
      );
      if (parentIndex == -1) {
        continue;
      }
      _drawPath(
        canvas,
        output[parentIndex].color,
        Path()
          ..moveTo(laneWidth * (parentIndex + 1), nodeY)
          ..lineTo(laneWidth * (circleIndex + 1), nodeY)
          ..moveTo(laneWidth * (parentIndex + 1), nodeY)
          ..quadraticBezierTo(
            laneWidth * (parentIndex + 1),
            laneHeight,
            laneWidth * (parentIndex + 1),
            laneHeight,
          ),
      );
    }

    if (inputIndex != -1) {
      _drawPath(
        canvas,
        input[inputIndex].color,
        Path()
          ..moveTo(laneWidth * (circleIndex + 1), 0)
          ..lineTo(laneWidth * (circleIndex + 1), nodeY),
      );
    }
    if (item.parentIds.isNotEmpty) {
      _drawPath(
        canvas,
        circleColor,
        Path()
          ..moveTo(laneWidth * (circleIndex + 1), nodeY)
          ..lineTo(laneWidth * (circleIndex + 1), laneHeight),
      );
    }

    _drawNode(canvas, circleIndex, circleColor);
  }

  void _drawPath(Canvas canvas, GitHistoryGraphColorId color, Path path) {
    canvas.drawPath(
      path,
      Paint()
        ..color = gitHistoryGraphColor(color) ?? AleraTokens.foregroundMuted
        ..strokeWidth = 1.5
        ..strokeCap = StrokeCap.round
        ..style = PaintingStyle.stroke,
    );
  }

  void _drawNode(Canvas canvas, int circleIndex, GitHistoryGraphColorId color) {
    final center = Offset(laneWidth * (circleIndex + 1), nodeY);
    final paint = Paint()
      ..color = gitHistoryGraphColor(color) ?? AleraTokens.foreground;
    final boundary =
        viewModel.kind == GitHistoryItemViewModelKind.incomingChanges ||
        viewModel.kind == GitHistoryItemViewModelKind.outgoingChanges;
    if (viewModel.kind == GitHistoryItemViewModelKind.head || boundary) {
      canvas.drawCircle(center, circleRadius + 3, paint);
      canvas.drawCircle(center, circleRadius, Paint()..color = AleraTokens.bg);
      if (boundary) {
        canvas.drawCircle(
          center,
          circleRadius + 1,
          Paint()
            ..color = paint.color
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1,
        );
      }
      return;
    }
    if (viewModel.historyItem.parentIds.length > 1) {
      canvas.drawCircle(center, circleRadius + 1, paint);
      canvas.drawCircle(
        center,
        circleRadius - 1.5,
        Paint()..color = AleraTokens.bg,
      );
      return;
    }
    canvas.drawCircle(center, circleRadius, paint);
  }

  @override
  bool shouldRepaint(covariant GitHistoryGraphPainter oldDelegate) {
    return oldDelegate.viewModel != viewModel;
  }
}

Color? gitHistoryGraphColor(GitHistoryGraphColorId? color) {
  return switch (color) {
    GitHistoryGraphColorId.ref => AleraTokens.success,
    GitHistoryGraphColorId.remoteRef => AleraTokens.info,
    GitHistoryGraphColorId.baseRef => AleraTokens.warning,
    GitHistoryGraphColorId.lane1 => AleraTokens.syntaxFunction,
    GitHistoryGraphColorId.lane2 => AleraTokens.syntaxKeyword,
    GitHistoryGraphColorId.lane3 => AleraTokens.syntaxLiteral,
    GitHistoryGraphColorId.lane4 => AleraTokens.syntaxOperator,
    GitHistoryGraphColorId.lane5 => AleraTokens.foregroundMuted,
    null => null,
  };
}

/// Pill badge for a ref attached to a commit. [onOpenActions] receives the
/// pointer's global position so callers can anchor a context menu.
class const GitRefBadge({
  super.key,
  required final GitHistoryItemRef itemRef,
  final ValueChanged<Offset>? onOpenActions,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final color =
        gitHistoryGraphColor(itemRef.color) ??
        switch (itemRef.category) {
          GitHistoryRefCategory.branches => AleraTokens.success,
          GitHistoryRefCategory.remoteBranches => AleraTokens.info,
          GitHistoryRefCategory.tags => AleraTokens.warning,
          GitHistoryRefCategory.commits => AleraTokens.accent,
          null => AleraTokens.accent,
        };
    final badge = DecoratedBox(
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(AleraTokens.radiusPill),
        border: Border.all(color: color.withValues(alpha: 0.72)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: AleraTokens.space6,
          vertical: AleraTokens.space2,
        ),
        child: Text(
          itemRef.name,
          maxLines: 1,
          overflow: .ellipsis,
          style: Theme.of(context).textTheme.labelSmall
              ?.copyWith(color: color, fontSize: 10, fontWeight: .w700),
        ),
      ),
    );
    final tooltipBadge = Tooltip(message: itemRef.name, child: badge);
    final onOpenActions = this.onOpenActions;
    if (onOpenActions == null) {
      return tooltipBadge;
    }
    return GestureDetector(
      behavior: .opaque,
      onSecondaryTapDown: (details) => onOpenActions(details.globalPosition),
      onLongPressStart: (details) => onOpenActions(details.globalPosition),
      child: tooltipBadge,
    );
  }
}
