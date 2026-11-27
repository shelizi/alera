/// A production-ready file system explorer for Flutter applications.
///
/// **Key Features:**
/// * **Virtualization:** Efficiently renders huge trees using flattening and virtual scrolling.
/// * **State Management:** Preserves selection and expansion states across data refreshes.
/// * **Interactivity:** Built-in keyboard navigation (arrows, shift-select), context menus, and drag-and-drop hooks.
/// * **Theming:** Fully customizable look via [DirectoryTreeThemeData], with sensible defaults for Light/Dark modes.
///
/// **Getting Started:**
/// 1. Create a [DirectoryTreeController] with your [TreeData].
/// 2. Display it using the all-in-one [DirectoryTreePanel] or build a custom view with [DirectoryTreeView].
///
/// See individual classes for detailed behavior documentation.
library;

export 'package:directory_tree/directory_tree.dart' hide folderSelection;

// ------------------- Controllers -------------------
export 'src/controller/directory_tree_controller.dart';
export 'src/controller/expansion_controller.dart';
export 'src/controller/selection_controller.dart';
export 'src/controller/tree_diff.dart';
export 'src/delegates/context_menu_delegate.dart';
export 'src/delegates/icon_provider.dart';
// ------------------- Delegates & APIs -------------------
export 'src/delegates/node_renderer.dart';
export 'src/models/commands.dart';
// ------------------- Models & State -------------------
// ------------------- Prebuilt Dialogs/Panes -------------------
export 'src/prebuilt/directory_tree_picker.dart';
// ------------------- Services & Utilities -------------------
export 'src/services/reveal_path.dart';
export 'src/services/selection_utils.dart';
export 'src/theme/defaults.dart';
// ------------------- Theming -------------------
export 'src/theme/directory_tree_theme.dart';
export 'src/widgets/directory_tree_panel.dart';
export 'src/widgets/directory_tree_toolbar.dart';
// ------------------- Core Widgets -------------------
export 'src/widgets/directory_tree_view.dart';
export 'src/widgets/selection_shortcuts.dart';
// ------------------- UI Components & Helpers -------------------
export 'src/widgets/tree_node_tile.dart';
