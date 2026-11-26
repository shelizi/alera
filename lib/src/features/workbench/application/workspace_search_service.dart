import 'package:alera/src/rust/api/workspace_search.dart' as native;

class const WorkspaceSearchService() {
  Future<native.WorkspaceSearchResult> search({
    required native.WorkspaceSearchOptions options,
    required String requestId,
  }) {
    return native.searchWorkspaceCancelable(
      options: options,
      requestId: requestId,
    );
  }

  /// Services without a streaming transport retain the complete-result
  /// behavior (including test doubles and other consumers).
  Stream<native.WorkspaceSearchBatch> searchBatches({
    required native.WorkspaceSearchOptions options,
    required String requestId,
  }) async* {
    final result = await search(options: options, requestId: requestId);
    yield native.WorkspaceSearchBatch(
      files: result.files,
      totalMatches: result.totalMatches,
      truncated: result.truncated,
      done: true,
    );
  }

  Future<native.WorkspaceReplacePreview> previewReplace({
    required native.WorkspaceReplaceOptions options,
    required String requestId,
  }) {
    return native.previewWorkspaceReplaceCancelable(
      options: options,
      requestId: requestId,
    );
  }

  Future<void> cancel({required String requestId}) =>
      native.cancelWorkspaceSearch(requestId: requestId);

  Future<native.WorkspaceReplaceResult> replaceMatches({
    required native.WorkspaceReplaceRequest request,
  }) {
    return native.replaceWorkspaceMatches(request: request);
  }
}

/// The desktop provider uses the generated native streaming transport.
class const NativeWorkspaceSearchService() extends WorkspaceSearchService {
  @override
  Stream<native.WorkspaceSearchBatch> searchBatches({
    required native.WorkspaceSearchOptions options,
    required String requestId,
  }) => native.searchWorkspaceStream(options: options, requestId: requestId);
}
