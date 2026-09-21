import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution dartLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('dart'),
          displayName: 'Dart',
          fileExtensions: const <String>['dart'],
          aliases: const <String>['dartlang'],
          parserProviderId: 'dart.tree-sitter',
          structuralParserDefaultEnabled: true,
          semanticProviderIds: const <String>['dart.analysis-server'],
          defaultSemanticProviderId: 'dart.analysis-server',
          capabilities: builtinLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'dart.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('dart')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'dart.analysis-server',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('dart')},
          capabilities: builtinNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['dart'],
          defaultArguments: const <String>['language-server', '--protocol=lsp'],
        ),
      ],
    );
