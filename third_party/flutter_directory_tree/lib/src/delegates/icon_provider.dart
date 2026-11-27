// lib/src/delegates/icon_provider.dart
import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/material.dart' show Icon, Icons;
import 'package:flutter/widgets.dart';
import 'package:path/path.dart' as p;

/// Strategy for resolving icons based on node state and file type.
///
/// **Why:** Allows the tree to be styled for different environments (IDE vs File Explorer)
/// without modifying core logic.
abstract class IconProvider {
  /// Abstract constant constructor.
  const IconProvider();

  /// Returns the icon displayed before the node title.
  ///
  /// Typically shows folder/file glyphs, potentially varying by file extension.
  Widget? leadingIcon(BuildContext context, VisibleNode node);

  /// Returns an optional icon displayed at the end of the row.
  ///
  /// Useful for status indicators like "synced", "error", or "modified".
  Widget? trailingIcon(BuildContext context, VisibleNode node) => null;
}

/// Default implementation using standard Material Design icons.
///
/// * Folders use [Icons.folder].
/// * Files use specific icons for `.dart`, `.json`, `.png`, etc.
/// * Unknown files fall back to [Icons.insert_drive_file].
class MaterialIconProvider extends IconProvider {
  /// Creates a provider that uses standard Material Icons.
  const MaterialIconProvider();

  @override
  Widget? leadingIcon(BuildContext context, VisibleNode node) {
    if (node.type == NodeType.folder) {
      return const Icon(Icons.folder, size: 18);
    }
    if (node.type == NodeType.root) {
      return const Icon(Icons.storage, size: 18);
    }

    // file
    final ext = p.extension(node.name).toLowerCase();
    final icon = switch (ext) {
      '.dart' => Icons.code,
      '.md' => Icons.article,
      '.json' => Icons.data_object,
      '.yaml' || '.yml' => Icons.description,
      '.png' || '.jpg' || '.jpeg' || '.gif' || '.svg' => Icons.image,
      '.txt' => Icons.description,
      _ => node.isVirtual ? Icons.cloud_queue : Icons.insert_drive_file,
    };
    return Icon(icon, size: 18);
  }
}
