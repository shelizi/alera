import '../application/language_semantic_adapter_factory.dart';
import '../application/language_server_runtime.dart';
import '../domain/language_provider_descriptor.dart';
import 'code_forge_language_server_runtime.dart';
import 'code_forge_semantic_provider_adapter.dart';

final class CodeForgeSemanticAdapterFactory
    implements LanguageSemanticAdapterFactory {
  const CodeForgeSemanticAdapterFactory({
    this.sourceTextReader,
    this.isWindows,
  });

  final CodeForgeSourceTextReader? sourceTextReader;
  final bool? isWindows;

  @override
  LanguageSemanticProviderBinding create({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
    required LanguageServerRuntimeSession session,
  }) {
    if (session is! CodeForgeLanguageServerSession) {
      throw ArgumentError.value(
        session,
        'session',
        'CodeForgeSemanticAdapterFactory requires a CodeForge runtime session.',
      );
    }
    final adapter = CodeForgeSemanticProviderAdapter(
      workspaceId: workspaceId,
      session: session,
      sourceTextReader: sourceTextReader,
      isWindows: isWindows,
    );
    return LanguageSemanticProviderBinding(
      documents: adapter,
      navigation: adapter,
    );
  }
}
