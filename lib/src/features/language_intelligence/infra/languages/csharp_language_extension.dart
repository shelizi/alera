import '../../domain/language_extension_contribution.dart';
import '../../domain/language_extension_descriptor.dart';
import '../../domain/language_id.dart';
import '../../domain/language_provider_descriptor.dart';
import 'language_extension_capabilities.dart';

final LanguageExtensionContribution csharpLanguageExtension =
    LanguageExtensionContribution(
      languages: <LanguageExtensionDescriptor>[
        LanguageExtensionDescriptor(
          id: LanguageId('csharp'),
          displayName: 'C#',
          fileExtensions: const <String>['cs', 'csx'],
          aliases: const <String>['cs', 'c#'],
          parserProviderId: 'csharp.tree-sitter',
          semanticProviderIds: const <String>['csharp.csharp-ls'],
          defaultSemanticProviderId: 'csharp.csharp-ls',
          capabilities: builtinLanguageCapabilities,
        ),
      ],
      providers: <LanguageProviderDescriptor>[
        LanguageProviderDescriptor(
          id: 'csharp.tree-sitter',
          kind: LanguageProviderKind.structuralParser,
          languages: <LanguageId>{LanguageId('csharp')},
          capabilities: builtinStructuralCapabilities,
          processScope: LanguageProviderProcessScope.document,
          launchPolicy: LanguageProviderLaunchPolicy.explicit,
        ),
        LanguageProviderDescriptor(
          id: 'csharp.csharp-ls',
          kind: LanguageProviderKind.semanticServer,
          languages: <LanguageId>{LanguageId('csharp')},
          capabilities: builtinNavigationCapabilities,
          processScope: LanguageProviderProcessScope.workspace,
          launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
          executableResolutionPolicy:
              LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
          executableCandidates: const <String>['csharp-ls'],
        ),
      ],
    );
