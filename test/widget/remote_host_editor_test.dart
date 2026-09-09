import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/features/settings/presentation/panes/remote_host_editor.dart';
import 'package:alera/src/features/remote_hosts/domain/ssh_target.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('remote host save button uses the click cursor', (tester) async {
    final aliasController = TextEditingController();
    final hostController = TextEditingController();
    final portController = TextEditingController(text: '22');
    final usernameController = TextEditingController();
    final installDirController = TextEditingController();
    addTearDown(aliasController.dispose);
    addTearDown(hostController.dispose);
    addTearDown(portController.dispose);
    addTearDown(usernameController.dispose);
    addTearDown(installDirController.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildAleraDarkTheme(),
        home: Scaffold(
          body: RemoteHostEditor(
            aliasController: aliasController,
            hostController: hostController,
            portController: portController,
            usernameController: usernameController,
            installDirController: installDirController,
            platform: '',
            arch: '',
            authKind: .agent,
            hasSelection: false,
            saving: false,
            planning: false,
            bootstrapping: false,
            onPlatformChanged: (_) {},
            onArchChanged: (_) {},
            onAuthKindChanged: (_) {},
            onSave: () {},
            onRemove: null,
            onPlan: null,
            onBootstrap: null,
            onCancel: null,
          ),
        ),
      ),
    );

    final saveButton = find.widgetWithText(FilledButton, 'Save');
    await tester.ensureVisible(saveButton);
    await tester.pump();

    final mouse = await tester.createGesture(kind: .mouse, pointer: 1);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: tester.getCenter(saveButton));
    await tester.pump();

    expect(
      RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1),
      SystemMouseCursors.click,
    );
  });

  testWidgets('renders remote host chrome in Traditional Chinese', (
    tester,
  ) async {
    final aliasController = TextEditingController();
    final hostController = TextEditingController();
    final portController = TextEditingController(text: '22');
    final usernameController = TextEditingController();
    final installDirController = TextEditingController();
    addTearDown(aliasController.dispose);
    addTearDown(hostController.dispose);
    addTearDown(portController.dispose);
    addTearDown(usernameController.dispose);
    addTearDown(installDirController.dispose);

    await tester.pumpWidget(
      MaterialApp(
        locale: const Locale('zh', 'TW'),
        supportedLocales: supportedAleraLocales,
        localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
          AleraLocalizationsDelegate(),
          ...GlobalMaterialLocalizations.delegates,
        ],
        theme: buildAleraDarkTheme(),
        home: Scaffold(
          body: RemoteHostEditor(
            aliasController: aliasController,
            hostController: hostController,
            portController: portController,
            usernameController: usernameController,
            installDirController: installDirController,
            platform: '',
            arch: '',
            authKind: .agent,
            hasSelection: false,
            saving: false,
            planning: false,
            bootstrapping: false,
            progress: const SshTargetBootstrapProgress(
              jobId: 'job-1',
              targetId: 'host-1',
              status: .installing,
              stage: 'installing',
              message: 'Remote runtime install started',
            ),
            onPlatformChanged: (_) {},
            onArchChanged: (_) {},
            onAuthKindChanged: (_) {},
            onSave: () {},
            onRemove: null,
            onPlan: null,
            onBootstrap: null,
            onCancel: null,
          ),
        ),
      ),
    );

    expect(find.text('連線'), findsOneWidget);
    expect(find.text('別名'), findsOneWidget);
    expect(find.text('驗證方式'), findsOneWidget);
    expect(find.text('執行環境初始化'), findsOneWidget);
    expect(find.text('依平台使用預設值'), findsOneWidget);
    expect(find.text('遠端執行環境已開始安裝'), findsOneWidget);
    expect(find.text('安裝中'), findsOneWidget);
  });
}
