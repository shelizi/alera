// lib/src/services/selection_utils.dart
import 'package:directory_tree/directory_tree.dart' as core
    show folderSelection;
import 'package:directory_tree/directory_tree.dart' show FolderSelection;
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';

export 'package:directory_tree/directory_tree.dart' show FolderSelection;

/// Computes the selection state (Checked, Unchecked, Indeterminate) for a folder.
///
/// **Why:** Useful for implementing "checkbox" style selection where selecting a parent
/// implies selecting children.
///
/// Returns a [FolderSelection] object indicating if all, some, or none of the
/// folder's descendants are in [controller.selection].
FolderSelection folderSelection(
  DirectoryTreeController controller,
  String folderId,
) {
  return core.folderSelection(
    data: controller.data,
    selectedIds: controller.selection.selectedIds,
    folderId: folderId,
  );
}
