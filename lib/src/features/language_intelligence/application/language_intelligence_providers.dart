import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_semantic_adapter_factory.dart';
import 'package:alera/src/features/language_intelligence/infra/git_nested_worktree_locator.dart';
import 'package:alera/src/features/language_intelligence/infra/local_workspace_marker_files.dart';
import 'package:alera/src/features/language_intelligence/infra/process_runner_lsp_process.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart' show Provider;
import 'package:riverpod_annotation/riverpod_annotation.dart';

import 'language_intelligence_activity.dart';
import 'language_intelligence_manager.dart';
import 'language_intelligence_status_port.dart';
import 'language_provider_registry.dart';
import 'language_semantic_adapter_factory.dart';
import 'language_server_runtime.dart';
import 'language_server_session_manager.dart';

part 'language_intelligence_providers.g.dart';

@Riverpod(keepAlive: true)
LanguageExtensionRegistry languageExtensionRegistry(Ref ref) =>
    createBuiltinLanguageExtensionRegistry();

final languageIntelligenceActivityProvider =
    Provider<LanguageIntelligenceActivityStore>((ref) {
      final store = LanguageIntelligenceActivityStore();
      ref.onDispose(store.dispose);
      return store;
    });

@Riverpod(keepAlive: true)
LanguageServerRuntimePort languageServerRuntime(Ref ref) =>
    CodeForgeLanguageServerRuntime(
      processStarter: processRunnerLspProcessStarter(
        ref.watch(processRunnerProvider),
      ),
      activityReporter: ref.watch(languageIntelligenceActivityProvider),
      nestedCheckouts: GitNestedWorktreeLocator(
        ref.watch(gitBackendProvider).listWorktrees,
      ),
    );

@Riverpod(keepAlive: true)
LanguageSemanticAdapterFactory languageSemanticAdapterFactory(Ref ref) =>
    const CodeForgeSemanticAdapterFactory();

@Riverpod(keepAlive: true)
LanguageServerSessionManager languageServerSessionManager(Ref ref) =>
    LanguageServerSessionManager(
      registry: ref.watch(languageExtensionRegistryProvider),
      runtime: ref.watch(languageServerRuntimeProvider),
      activityReporter: ref.watch(languageIntelligenceActivityProvider),
    );

@Riverpod(keepAlive: true)
LanguageIntelligenceManager languageIntelligenceManager(Ref ref) =>
    LanguageIntelligenceManager(
      registry: ref.watch(languageExtensionRegistryProvider),
      sessions: ref.watch(languageServerSessionManagerProvider),
      semanticAdapterFactory: ref.watch(languageSemanticAdapterFactoryProvider),
      markerFiles: const LocalWorkspaceMarkerFiles(),
    );

@Riverpod(keepAlive: true)
LanguageIntelligenceStatusPort languageIntelligenceStatusPort(Ref ref) =>
    RuntimeLanguageIntelligenceStatusPort(
      runtime: ref.watch(languageServerRuntimeProvider),
    );
