import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution pythonLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('python'),
          displayName: 'Python',
          fileExtensions: const <String>['py', 'pyw', 'pyi'],
          aliases: const <String>['py'],
          parserProviderId: 'python.tree-sitter',
          structuralParserDefaultEnabled: true,
          // Measured on a 7,000-file project: pyrefly is the only one that
          // resolves overrides reached through untyped call sites and returns
          // exactly the implementations; ty is the fastest but still 0.0.x;
          // pyright stays for low-memory machines and has no implementation.
          semanticProviderIds: const <String>[
            'python.pyrefly',
            'python.ty',
            'python.pyright',
          ],
          defaultSemanticProviderId: 'python.pyrefly',
          capabilities: builtinImplementationLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'python.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('python')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'python.pyrefly',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('python')},
          capabilities: builtinImplementationNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['pyrefly'],
          defaultArguments: const <String>['lsp'],
        ),
        LanguageProviderDescriptor(
          id: 'python.ty',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('python')},
          capabilities: builtinImplementationNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['ty'],
          defaultArguments: const <String>['server'],
        ),
        LanguageProviderDescriptor(
          id: 'python.pyright',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('python')},
          capabilities: builtinNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['pyright-langserver'],
          defaultArguments: const <String>['--stdio'],
        ),
      ],
    );
