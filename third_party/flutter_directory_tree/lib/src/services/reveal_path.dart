// lib/src/services/reveal_path.dart
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';

export 'package:directory_tree/directory_tree.dart'
    show ancestorChain, findByVirtualPath;

/// Convenience wrapper to reveal and select a node in one step.
///
/// See [DirectoryTreeController.reveal] for details.
Future<void> revealAndSelect(
  DirectoryTreeController controller, {
  String? nodeId,
  String? virtualPath,
}) {
  return controller.reveal(
      nodeId: nodeId, virtualPath: virtualPath, select: true);
}
