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
          semanticProviderIds: const <String>['python.pyright'],
          defaultSemanticProviderId: 'python.pyright',
          capabilities: builtinLanguageCapabilities,
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
