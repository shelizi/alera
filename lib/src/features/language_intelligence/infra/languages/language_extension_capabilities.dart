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

/// Navigation for servers verified to answer `textDocument/implementation`.
/// Pyright and the free intelephense tier reject or ignore it, so they keep
/// [builtinNavigationCapabilities].
const Set<LanguageCapability> builtinImplementationNavigationCapabilities =
    <LanguageCapability>{
      ...builtinNavigationCapabilities,
      LanguageCapability.implementation,
    };

const Set<LanguageCapability> builtinImplementationLanguageCapabilities =
    <LanguageCapability>{
      ...builtinStructuralCapabilities,
      ...builtinImplementationNavigationCapabilities,
    };
