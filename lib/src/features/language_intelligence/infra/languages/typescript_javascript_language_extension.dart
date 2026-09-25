import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution typescriptJavascriptLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('typescript'),
          displayName: 'TypeScript',
          fileExtensions: const <String>['ts', 'tsx', 'mts', 'cts'],
          aliases: const <String>['ts'],
          syntaxLanguageIdsByExtension: const <String, String>{'tsx': 'tsx'},
          parserProviderId: 'typescript.tree-sitter',
          structuralParserDefaultEnabled: true,
          semanticProviderIds: const <String>[
            'typescript-javascript.typescript-language-server',
          ],
          defaultSemanticProviderId:
              'typescript-javascript.typescript-language-server',
          capabilities: builtinImplementationLanguageCapabilities,
        ),
        LanguageExtensionDescriptor(
          id: LanguageId('javascript'),
          displayName: 'JavaScript',
          fileExtensions: const <String>['js', 'jsx', 'mjs', 'cjs'],
          aliases: const <String>['js'],
          syntaxLanguageIdsByExtension: const <String, String>{'jsx': 'jsx'},
          parserProviderId: 'javascript.tree-sitter',
          structuralParserDefaultEnabled: true,
          semanticProviderIds: const <String>[
            'typescript-javascript.typescript-language-server',
          ],
          defaultSemanticProviderId:
              'typescript-javascript.typescript-language-server',
          capabilities: builtinImplementationLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'typescript.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('typescript')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'javascript.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('javascript')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'typescript-javascript.typescript-language-server',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{
            LanguageId('typescript'),
            LanguageId('javascript'),
          },
          capabilities: builtinImplementationNavigationCapabilities,
          processScope: LanguageProviderProcessScope.sharedWorkspaceFamily,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['typescript-language-server'],
          defaultArguments: const <String>['--stdio'],
        ),
      ],
    );
