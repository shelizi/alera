import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/core/build_flavor.dart';
import 'package:alera/src/design_system/feedback/alera_toast_host.dart';
import 'package:alera/src/features/app_window/presentation/app_window_lifecycle_scope.dart';
import 'package:alera/src/features/ai_dictation/presentation/ai_dictation_download_bootstrap.dart';
import 'package:alera/src/features/diagnostics/presentation/diagnostics_settings_scope.dart';
import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:alera/src/features/text_actions/presentation/text_actions_scope.dart';
import 'package:alera/src/features/updater/presentation/update_availability_watch.dart';
import 'package:alera/src/features/desktop_presence/presentation/desktop_presence_scope.dart';
import 'package:alera/src/features/runtime_host/presentation/runtime_host_quit_gate_scope.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class const AleraApp({super.key}) extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final language = ref.watch(
      settingsControllerProvider.select((settings) => settings.general.language),
    );
    return MaterialApp(
      title: kAleraAppName,
      locale: language == AppLanguage.system
          ? null
          : resolveAleraLocale(language, null),
      supportedLocales: supportedAleraLocales,
      localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
        AleraLocalizationsDelegate(),
        ...GlobalMaterialLocalizations.delegates,
      ],
      localeResolutionCallback: (locale, supportedLocales) =>
          resolveAleraLocale(AppLanguage.system, locale),
      home: const RuntimeHostQuitGateScope(
        child: DesktopPresenceScope(child: AleraShellPage()),
      ),
      debugShowCheckedModeBanner: false,
      builder: (context, child) {
        return AiDictationDownloadBootstrap(
          child: AppWindowLifecycleScope(
            child: DiagnosticsSettingsScope(
              child: UpdateAvailabilityWatch(
                child: Stack(
                  children: <Widget>[
                    TextActionsScope(child: child ?? const SizedBox.shrink()),
                    const AleraToastHost(),
                  ],
                ),
              ),
            ),
          ),
        );
      },
      theme: aleraDarkTheme,
      darkTheme: aleraDarkTheme,
      themeMode: .dark,
    );
  }
}
