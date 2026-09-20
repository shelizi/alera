import '../application/language_provider_registry.dart';
import '../domain/language_extension_contribution.dart';
import 'languages/csharp_language_extension.dart';
import 'languages/go_language_extension.dart';
import 'languages/php_language_extension.dart';
import 'languages/python_language_extension.dart';
import 'languages/rust_language_extension.dart';
import 'languages/typescript_javascript_language_extension.dart';

List<LanguageExtensionContribution> get builtinLanguageExtensions =>
    <LanguageExtensionContribution>[
      csharpLanguageExtension,
      pythonLanguageExtension,
      rustLanguageExtension,
      goLanguageExtension,
      phpLanguageExtension,
      typescriptJavascriptLanguageExtension,
    ];

LanguageExtensionRegistry createBuiltinLanguageExtensionRegistry() {
  final registry = LanguageExtensionRegistry();
  registerBuiltinLanguageExtensions(registry);
  return registry;
}

void registerBuiltinLanguageExtensions(LanguageExtensionRegistry registry) {
  for (final contribution in builtinLanguageExtensions) {
    registry.registerContribution(contribution);
  }
}
