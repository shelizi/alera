import '../../domain/language_capability.dart';

const Set<LanguageCapability> builtinStructuralCapabilities =
    <LanguageCapability>{
      LanguageCapability.syntax,
      LanguageCapability.documentSymbols,
      LanguageCapability.folding,
    };

const Set<LanguageCapability> builtinNavigationCapabilities =
    <LanguageCapability>{
      LanguageCapability.definition,
      LanguageCapability.references,
    };

const Set<LanguageCapability> builtinLanguageCapabilities =
    <LanguageCapability>{
      ...builtinStructuralCapabilities,
      ...builtinNavigationCapabilities,
    };
