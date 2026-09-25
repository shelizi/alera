import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution rustLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('rust'),
          displayName: 'Rust',
          fileExtensions: const <String>['rs'],
          aliases: const <String>['rs'],
          parserProviderId: 'rust.tree-sitter',
          structuralParserDefaultEnabled: true,
          semanticProviderIds: const <String>['rust.rust-analyzer'],
          defaultSemanticProviderId: 'rust.rust-analyzer',
          capabilities: builtinImplementationLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'rust.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('rust')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'rust.rust-analyzer',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('rust')},
          capabilities: builtinImplementationNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['rust-analyzer'],
        ),
      ],
    );
