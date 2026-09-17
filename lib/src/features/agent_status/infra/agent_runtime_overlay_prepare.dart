part of 'agent_runtime_overlay_service.dart';

extension _AgentRuntimeOverlayPrepare on AgentRuntimeOverlayService {
  Future<AgentRuntimeOverlayPreparation> _prepareOverlay({
    required String agentKey,
    required String terminalSessionId,
    required String publicEnvKey,
    required String overlayEnvKey,
    required String sourceEnvKey,
    required String defaultSourcePath,
    required String managedSubdirectory,
    required Map<String, String> managedFiles,
  }) async {
    final source = _resolveSource(
      publicEnvKey: publicEnvKey,
      overlayEnvKey: overlayEnvKey,
      sourceEnvKey: sourceEnvKey,
      defaultSourcePath: defaultSourcePath,
    );
    if (source.isExplicit && !_sourceExists(source.path)) {
      return AgentRuntimeOverlayPreparation(
        sourcePath: source.path,
        environment: <String, String>{publicEnvKey: source.path},
      );
    }

    final support = await _applicationSupportDirectory();
    final root = _overlayRoot(support, agentKey);
    final overlay = _overlayDirectory(root, terminalSessionId);
    late final native.AgentRuntimeOverlayResult result;
    try {
      result = await _nativePreparer(
        request: native.AgentRuntimeOverlayRequest(
          overlayRoot: root,
          overlayPath: overlay.path,
          mirrorPath: null,
          sourcePath: source.path,
          managedSubdirectory: managedSubdirectory,
          managedFileNames: managedFiles.keys.toList(growable: false),
          managedFiles: <native.AgentRuntimeOverlayManagedFile>[
            for (final entry in managedFiles.entries)
              native.AgentRuntimeOverlayManagedFile(
                path: p.join(overlay.path, managedSubdirectory, entry.key),
                allowedRoot: overlay.path,
                content: entry.value,
                writeMode: native.AgentRuntimeOverlayWriteMode.replace,
                executable: false,
              ),
          ],
        ),
      );
    } catch (_) {
      if (source.isExplicit) {
        return AgentRuntimeOverlayPreparation(
          sourcePath: source.path,
          environment: <String, String>{publicEnvKey: source.path},
        );
      }
      return const AgentRuntimeOverlayPreparation(
        environment: <String, String>{},
      );
    }

    return AgentRuntimeOverlayPreparation(
      overlayPath: overlay.path,
      sourcePath: result.sourceExists ? source.path : null,
      environment: <String, String>{
        publicEnvKey: overlay.path,
        overlayEnvKey: overlay.path,
        if (result.sourceExists) sourceEnvKey: source.path,
      },
    );
  }
}
