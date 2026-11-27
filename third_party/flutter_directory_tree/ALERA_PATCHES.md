# Alera local patch

Source: the official pub.dev flutter_directory_tree 1.0.0 release archive
(https://pub.dev/api/archives/flutter_directory_tree-1.0.0.tar.gz).
SHA-256: 6f0e523dceb049a70ddfdd07a4c4aba706878ce9a216271b54a5ea4fb3ed4958,
verified against the previously locked hosted dependency before extraction.

Keep the upstream library, tests, documentation, pubspec and MIT LICENSE.
Platform examples and generated output are not needed and are omitted.

Local patch: DirectoryTreeController.lazyLookupIndexes (default false)
defers independent path and entry-ID indexes for immutable TreeData consumers;
on rebuild, retain expansion/selection through map membership, invalidate
lookup caches before observers, and create the corresponding index only on
first lookup. Alera opts in because it navigates through its native node-ID
bindings. Full immutable tree snapshots and public navigation APIs remain.

Tests live in test/unit/workspace_explorer_tree_publication_test.dart in the
Alera root. Re-evaluate this override when upgrading upstream.

Additional opt-in patch: DirectoryTreeController.immutableVisibleRows
(default false) supports immutable flattened lists with unique row IDs.
Identical list publications keep their visible index; DirectoryTreeView
uses the existing unmodifiable list and preserves the scroll anchor
without constructing a full LIS edit script. Alera supplies an
incremental root-branch flatten strategy. Existing default controller
and view behavior is retained. Tests cover scroll anchoring and selection
in both modes; Alera unit tests cover incremental/reference row parity.
