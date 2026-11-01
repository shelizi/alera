import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/features/settings/infra/alera_cli_registration_service.dart';
import 'package:alera/src/features/settings/presentation/panes/agents_cli_skill_control.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('up-to-date CLI registration offers no update action', (
    tester,
  ) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          aleraCliRegistrationServiceProvider.overrideWithValue(
            _FakeAleraCliRegistrationService(statusResult: _registeredStatus),
          ),
        ],
        child: MaterialApp(
          theme: buildAleraDarkTheme(),
          home: const Scaffold(body: AleraCliRegistrationControl()),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(find.text('Update'), findsNothing);
    expect(find.text('Registered'), findsOneWidget);
    final action = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Registered'),
    );
    expect(action.onPressed, isNull);
  });

  testWidgets('stale CLI registration offers an update action', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          aleraCliRegistrationServiceProvider.overrideWithValue(
            _FakeAleraCliRegistrationService(statusResult: _staleStatus),
          ),
        ],
        child: MaterialApp(
          theme: buildAleraDarkTheme(),
          home: const Scaffold(body: AleraCliRegistrationControl()),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final action = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Update'),
    );
    expect(action.onPressed, isNotNull);
  });

  testWidgets('registration control surfaces refresh failures', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          aleraCliRegistrationServiceProvider.overrideWithValue(
            _FakeAleraCliRegistrationService(
              statusError: StateError('missing'),
            ),
          ),
        ],
        child: MaterialApp(
          theme: buildAleraDarkTheme(),
          home: const Scaffold(
            body: Align(
              alignment: Alignment.topLeft,
              child: AleraCliRegistrationControl(),
            ),
          ),
        ),
      ),
    );

    await tester.pump();
    await tester.pump();

    expect(find.textContaining('Registration check failed'), findsOneWidget);
    expect(find.textContaining('missing'), findsOneWidget);
  });

  testWidgets('registration control surfaces install failures', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          aleraCliRegistrationServiceProvider.overrideWithValue(
            _FakeAleraCliRegistrationService(
              installError: StateError('permission denied'),
            ),
          ),
        ],
        child: MaterialApp(
          theme: buildAleraDarkTheme(),
          home: const Scaffold(
            body: Align(
              alignment: Alignment.topLeft,
              child: AleraCliRegistrationControl(),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    await tester.tap(find.text('Register'));
    await tester.pump();
    await tester.pump();

    expect(find.textContaining('Registration failed'), findsOneWidget);
    expect(find.textContaining('permission denied'), findsOneWidget);
  });
}

class _FakeAleraCliRegistrationService({
  final Object? statusError,
  final Object? installError,
  final AleraCliRegistrationStatus? statusResult,
}) extends AleraCliRegistrationService {
  this : super(processRunner: _NoopProcessRunner());

  @override
  Future<AleraCliRegistrationStatus> status() async {
    final error = statusError;
    if (error != null) {
      throw error;
    }
    return statusResult ?? _notRegisteredStatus;
  }

  @override
  Future<AleraCliRegistrationStatus> installOrUpdate() async {
    final error = installError;
    if (error != null) {
      throw error;
    }
    return _notRegisteredStatus;
  }
}

class _NoopProcessRunner implements ProcessRunner {
  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) {
    throw UnimplementedError();
  }

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) {
    throw UnimplementedError();
  }
}

const _notRegisteredStatus = AleraCliRegistrationStatus(
  commandName: 'alera',
  commandPath: '/Users/test/.local/bin/alera',
  pathDirectory: '/Users/test/.local/bin',
  pathConfigured: false,
  launcherPath: '/Applications/Alera.app/alera',
  installMethod: .wrapper,
  state: .notInstalled,
  detail: 'Register the Alera command to use it from terminals and agents.',
);

const _registeredStatus = AleraCliRegistrationStatus(
  commandName: 'alera',
  commandPath: '/Users/test/.local/bin/alera',
  pathDirectory: '/Users/test/.local/bin',
  pathConfigured: true,
  launcherPath: '/Applications/Alera.app/alera',
  installMethod: .wrapper,
  state: .installed,
  detail: 'Registered at /Users/test/.local/bin/alera.',
);

const _staleStatus = AleraCliRegistrationStatus(
  commandName: 'alera',
  commandPath: '/Users/test/.local/bin/alera',
  pathDirectory: '/Users/test/.local/bin',
  pathConfigured: true,
  launcherPath: '/Applications/Alera.app/alera',
  installMethod: .wrapper,
  state: .stale,
  detail: 'Registered command points to an older Alera launcher.',
);
