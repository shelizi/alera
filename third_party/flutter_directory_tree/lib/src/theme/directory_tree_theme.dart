// lib/src/theme/directory_tree_theme.dart
import 'package:flutter/widgets.dart';

/// Defines the visual properties of the directory tree.
///
/// Use [DirectoryTreeTheme] to inject this into the widget tree.
class DirectoryTreeThemeData {
  /// Creates a theme configuration.
  ///
  /// * [rowHeight]: Fixed height for every row (default 28.0).
  /// * [indent]: Horizontal pixel offset per depth level (default 16.0).
  /// * [indentGuides]: Whether to draw vertical lines indicating depth (not yet implemented in all views).
  /// * [hoverColor]: Background color when the mouse hovers a row.
  /// * [selectionColor]: Background color for selected rows.
  /// * [focusColor]: Background color when a selected row has input focus.
  /// * [animationDuration]: Speed of expand/collapse animations.
  const DirectoryTreeThemeData({
    this.rowHeight = 28.0,
    this.indent = 16.0,
    this.indentGuides = true,
    this.hoverColor,
    this.selectionColor,
    this.focusColor,
    this.guideColor,
    this.animationDuration = const Duration(milliseconds: 120),
    this.roundedCorners = true,
  });

  /// Height of a single row in logical pixels.
  final double rowHeight;

  /// Horizontal indentation in logical pixels per tree level.
  final double indent;

  /// Whether to draw guide lines connecting parents to children (not yet implemented).
  final bool indentGuides;

  /// Background color of a row when the mouse pointer is hovering over it.
  final Color? hoverColor;

  /// Background color of a row when it is selected.
  final Color? selectionColor;

  /// Background color of a selected row when the tree has input focus.
  final Color? focusColor;

  /// Color of the indentation guide lines.
  final Color? guideColor;

  /// Duration for the expansion/collapse animation.
  final Duration animationDuration;

  /// Whether to round the corners of the row background decoration.
  final bool roundedCorners;

  /// Creates a copy of this theme but with the given fields replaced with the new values.
  DirectoryTreeThemeData copyWith({
    double? rowHeight,
    double? indent,
    bool? indentGuides,
    Color? hoverColor,
    Color? selectionColor,
    Color? focusColor,
    Color? guideColor,
    Duration? animationDuration,
    bool? roundedCorners,
  }) {
    return DirectoryTreeThemeData(
      rowHeight: rowHeight ?? this.rowHeight,
      indent: indent ?? this.indent,
      indentGuides: indentGuides ?? this.indentGuides,
      hoverColor: hoverColor ?? this.hoverColor,
      selectionColor: selectionColor ?? this.selectionColor,
      focusColor: focusColor ?? this.focusColor,
      guideColor: guideColor ?? this.guideColor,
      animationDuration: animationDuration ?? this.animationDuration,
      roundedCorners: roundedCorners ?? this.roundedCorners,
    );
  }
}

/// InheritedWidget that propagates [DirectoryTreeThemeData] down the tree.
///
/// Access using [DirectoryTreeTheme.of(context)].
class DirectoryTreeTheme extends InheritedWidget {
  /// Defines the theme data for the widget subtree.
  const DirectoryTreeTheme({
    super.key,
    required this.data,
    required super.child,
  });

  /// The configuration data for the theme.
  final DirectoryTreeThemeData data;

  /// Retrieves the nearest [DirectoryTreeThemeData] from the widget tree.
  ///
  /// Returns a default theme if none is found.
  static DirectoryTreeThemeData of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<DirectoryTreeTheme>()?.data ??
      const DirectoryTreeThemeData();

  @override
  bool updateShouldNotify(covariant DirectoryTreeTheme oldWidget) =>
      data != oldWidget.data;
}
