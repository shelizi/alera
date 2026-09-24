import 'dart:async';

import 'package:alera/src/features/language_intelligence/application/language_intelligence_providers.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

final workspaceLanguageIntelligencePrewarmCoordinatorProvider = Provider<void>((
  ref,
) {
  final manager = ref.watch(languageIntelligenceManagerProvider);
  String? retainedWorkspaceId;

  void synchronize() {
    final state = ref.read(workbenchControllerProvider);
    final workspace = state.bootstrapped ? state.activeWorkspace : null;
    final previousWorkspaceId = retainedWorkspaceId;
    final nextWorkspaceId = workspace?.id;
    retainedWorkspaceId = nextWorkspaceId;

    if (previousWorkspaceId != null && previousWorkspaceId != nextWorkspaceId) {
      manager.releaseWorkspacePrewarm(previousWorkspaceId);
    }
    if (workspace == null) return;

    final hostId = workspace.hostId.trim();
    if (hostId.isNotEmpty && hostId != 'local') {
      manager.releaseWorkspacePrewarm(workspace.id);
      return;
    }

    final settings = ref
        .read(settingsControllerProvider)
        .editor
        .languageIntelligence;
    unawaited(
      manager.prewarmWorkspace(
        workspaceId: workspace.id,
        workspaceRoot: workspace.path,
        settings: settings,
        target: LanguageServerTarget.localWorkspace,
      ),
    );
  }

  ref.listen(
    workbenchControllerProvider.select((state) {
      final workspace = state.activeWorkspace;
      return (
        bootstrapped: state.bootstrapped,
        workspaceId: workspace?.id,
        workspacePath: workspace?.path,
        hostId: workspace?.hostId,
      );
    }),
    (_, _) => synchronize(),
    fireImmediately: true,
  );
  ref.listen(
    settingsControllerProvider.select(
      (settings) => settings.editor.languageIntelligence,
    ),
    (_, _) => synchronize(),
  );
  ref.onDispose(() {
    final workspaceId = retainedWorkspaceId;
    if (workspaceId != null) {
      manager.releaseWorkspacePrewarm(workspaceId);
    }
  });
});
