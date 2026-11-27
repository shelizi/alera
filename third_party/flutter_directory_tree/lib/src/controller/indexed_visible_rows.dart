import 'package:directory_tree/directory_tree.dart';

/// Optional immutable, unique-ID row snapshot with its own visible index.
///
/// Controllers opting into immutableVisibleRows can delegate ID lookup
/// rather than building a duplicate full-list index. Implementations must
/// return -1 for absent rows and use the supplied current graph to resolve
/// IDs. The graph may have changed without changing the visible snapshot.
abstract interface class IndexedVisibleRows implements List<VisibleNode> {
  /// Returns the visible index of [nodeId] in [data], or -1 if absent.
  int indexOfVisibleNode(String nodeId, TreeData data);
}
