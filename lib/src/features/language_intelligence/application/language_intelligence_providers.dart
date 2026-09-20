import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:alera/src/features/language_intelligence/infra/code_forge_language_server_runtime.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

import 'language_intelligence_status_port.dart';
import 'language_provider_registry.dart';
import 'language_server_runtime.dart';

part 'language_intelligence_providers.g.dart';

@Riverpod(keepAlive: true)
LanguageExtensionRegistry languageExtensionRegistry(Ref ref) =>
    createBuiltinLanguageExtensionRegistry();

@Riverpod(keepAlive: true)
LanguageServerRuntimePort languageServerRuntime(Ref ref) =>
    CodeForgeLanguageServerRuntime();

@Riverpod(keepAlive: true)
LanguageIntelligenceStatusPort languageIntelligenceStatusPort(Ref ref) =>
    RuntimeLanguageIntelligenceStatusPort(
      runtime: ref.watch(languageServerRuntimeProvider),
    );
