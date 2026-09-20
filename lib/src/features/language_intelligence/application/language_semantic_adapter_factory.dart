import '../domain/language_provider_descriptor.dart';
import 'language_document_session_port.dart';
import 'language_navigation_port.dart';
import 'language_server_runtime.dart';

final class LanguageSemanticProviderBinding {
  const LanguageSemanticProviderBinding({
    required this.documents,
    required this.navigation,
  });

  final LanguageDocumentSessionPort documents;
  final LanguageNavigationPort navigation;
}

abstract interface class LanguageSemanticAdapterFactory {
  LanguageSemanticProviderBinding create({
    required String workspaceId,
    required LanguageProviderDescriptor provider,
    required LanguageServerRuntimeSession session,
  });
}
