import '../domain/language_intelligence_settings.dart';
import '../domain/language_provider_descriptor.dart';

enum LanguageServerTarget { localWorkspace, remoteWorkspace }

sealed class LanguageServerExecutableResolution {
  const LanguageServerExecutableResolution();
}

final class LanguageServerExecutableResolved
    extends LanguageServerExecutableResolution {
  const LanguageServerExecutableResolved(this.executable);

  final String executable;
}

final class LanguageServerExecutableMissing
    extends LanguageServerExecutableResolution {
  const LanguageServerExecutableMissing({required this.reason});

  final String reason;
}

abstract interface class LanguageServerRuntimeSession {}

final class LanguageServerExit {
  const LanguageServerExit({this.exitCode, this.error});

  final int? exitCode;
  final Object? error;
}

final class LanguageServerWorkProgress {
  const LanguageServerWorkProgress({
    required this.token,
    this.title,
    this.message,
    this.percentage,
    this.done = false,
  });

  final String token;
  final String? title;
  final String? message;
  final double? percentage;
  final bool done;
}

abstract interface class LanguageServerProgressRuntimePort {
  Stream<LanguageServerWorkProgress> observeProgress(
    LanguageServerRuntimeSession session,
  );
}

final class LanguageServerRuntimeStartRequest {
  LanguageServerRuntimeStartRequest({
    required this.provider,
    required this.executable,
    required this.workspaceRoot,
    required this.target,
    Iterable<String> arguments = const <String>[],
    Map<String, String> environment = const <String, String>{},
  }) : arguments = List<String>.unmodifiable(arguments),
       environment = Map<String, String>.unmodifiable(environment);

  final LanguageProviderDescriptor provider;
  final String executable;
  final String workspaceRoot;
  final LanguageServerTarget target;
  final List<String> arguments;
  final Map<String, String> environment;
}

abstract interface class LanguageServerRuntimePort {
  Future<LanguageServerExecutableResolution> resolveExecutable({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
    required LanguageServerTarget target,
  });

  Future<LanguageServerRuntimeSession> start(
    LanguageServerRuntimeStartRequest request,
  );

  Future<void> stop(LanguageServerRuntimeSession session);

  Stream<LanguageServerExit> observeExit(LanguageServerRuntimeSession session);
}
