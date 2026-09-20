import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'PHP defaults to cross-platform Intelephense and keeps Phpactor optional',
    () {
      final registry = createBuiltinLanguageExtensionRegistry();
      final php = registry.languageForId('php')!;

      expect(php.defaultSemanticProviderId, 'php.intelephense');
      expect(php.semanticProviderIds, <String>[
        'php.intelephense',
        'php.phpactor',
      ]);

      final intelephense = registry.provider('php.intelephense')!;
      expect(intelephense.kind, LanguageProviderKind.semanticServer);
      expect(intelephense.executableCandidates, <String>['intelephense']);
      expect(intelephense.defaultArguments, <String>['--stdio']);
      expect(
        intelephense.capabilities,
        containsAll(<LanguageCapability>[
          LanguageCapability.definition,
          LanguageCapability.references,
        ]),
      );

      final phpactor = registry.provider('php.phpactor')!;
      expect(phpactor.executableCandidates, <String>['phpactor']);
      expect(phpactor.defaultArguments, <String>['language-server']);
    },
  );
}
