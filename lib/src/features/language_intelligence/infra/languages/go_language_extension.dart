import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution goLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('go'),
          displayName: 'Go',
          fileExtensions: const <String>['go'],
          aliases: const <String>['golang'],
          parserProviderId: 'go.tree-sitter',
          semanticProviderIds: const <String>['go.gopls'],
          defaultSemanticProviderId: 'go.gopls',
          capabilities: builtinLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'go.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('go')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'go.gopls',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('go')},
          capabilities: builtinNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['gopls'],
          defaultArguments: const <String>['serve'],
        ),
      ],
    );
