import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution phpLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('php'),
          displayName: 'PHP',
          fileExtensions: const <String>['php', 'phtml'],
          parserProviderId: 'php.tree-sitter',
          semanticProviderIds: const <String>['php.phpactor'],
          defaultSemanticProviderId: 'php.phpactor',
          capabilities: builtinLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'php.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('php')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'php.phpactor',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('php')},
          capabilities: builtinNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['phpactor'],
          defaultArguments: const <String>['language-server'],
        ),
      ],
    );
