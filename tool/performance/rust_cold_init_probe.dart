// ignore_for_file: avoid_print, implementation_imports

/// S1 process-cold Rust initialization profiler.
///
/// Every invocation measures exactly one scenario. Run each scenario in a fresh
/// `flutter test` process; do not loop scenarios in-process because RustLib.init
/// is intentionally one-shot state.
///
/// Example:
///   dart run tool/performance/rust_cold_init_probe.dart \
///     --scenario=alera_preopened \
///     --alera-library=C:/path/to/alera_native.dll
library;

import 'dart:convert';
import 'dart:io';

import 'package:alera/src/rust/api/agent_descriptors.dart' as alera_api;
import 'package:alera/src/rust/frb_generated.dart' as alera_rust;
import 'package:code_forge/src/rust/api/editor.dart' as code_forge_api;
import 'package:code_forge/src/rust/frb_generated.dart' as code_forge_rust;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

var _scenario = 'sequential_default';
var _aleraLibraryPath = '';
var _codeForgeLibraryPath = '';

int _rss() => ProcessInfo.currentRss;

typedef _Report = Map<String, Object?>;

Future<int> _measureAsync(Future<void> Function() operation) async {
  final watch = Stopwatch()..start();
  await operation();
  watch.stop();
  return watch.elapsedMicroseconds;
}

int _measureSync(void Function() operation) {
  final watch = Stopwatch()..start();
  operation();
  watch.stop();
  return watch.elapsedMicroseconds;
}

int _firstAleraCall() => _measureSync(() {
  final id = alera_api.canonicalAgentId(id: 'codex');
  if (id == null || id.isEmpty) {
    throw StateError('Alera first native call returned an empty agent id.');
  }
});

int _firstCodeForgeCall() => _measureSync(() {
  final visible = code_forge_api.visibleLineRangeUnwrapped(
    totalLines: 100,
    viewTop: 0,
    viewBottom: 160,
    lineHeight: 16,
  );
  if (visible.firstLine != 0 || visible.lastLine < visible.firstLine) {
    throw StateError('CodeForge first native call returned an invalid range.');
  }
});

Future<_Report> _aleraDefault() async {
  final rssBefore = _rss();
  final initMicros = await _measureAsync(alera_rust.RustLib.init);
  final rssAfterInit = _rss();
  final firstCallMicros = _firstAleraCall();
  return <String, Object?>{
    'aleraInitMicros': initMicros,
    'aleraFirstCallMicros': firstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterInitBytes': rssAfterInit,
    'rssAfterFirstCallBytes': _rss(),
  };
}

Future<_Report> _codeForgeDefault() async {
  final rssBefore = _rss();
  final initMicros = await _measureAsync(code_forge_rust.RustLib.init);
  final rssAfterInit = _rss();
  final firstCallMicros = _firstCodeForgeCall();
  return <String, Object?>{
    'codeForgeInitMicros': initMicros,
    'codeForgeFirstCallMicros': firstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterInitBytes': rssAfterInit,
    'rssAfterFirstCallBytes': _rss(),
  };
}

Future<_Report> _sequentialDefault() async {
  final rssBefore = _rss();
  final both = Stopwatch()..start();
  final aleraInitMicros = await _measureAsync(alera_rust.RustLib.init);
  final rssAfterAleraInit = _rss();
  final codeForgeInitMicros = await _measureAsync(code_forge_rust.RustLib.init);
  both.stop();
  final rssAfterBothInit = _rss();
  final aleraFirstCallMicros = _firstAleraCall();
  final codeForgeFirstCallMicros = _firstCodeForgeCall();
  return <String, Object?>{
    'aleraInitMicros': aleraInitMicros,
    'codeForgeInitMicros': codeForgeInitMicros,
    'bothReadyMicros': both.elapsedMicroseconds,
    'aleraFirstCallMicros': aleraFirstCallMicros,
    'codeForgeFirstCallMicros': codeForgeFirstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterAleraInitBytes': rssAfterAleraInit,
    'rssAfterBothInitBytes': rssAfterBothInit,
    'rssAfterFirstCallsBytes': _rss(),
  };
}

Future<_Report> _concurrentDefault() async {
  final rssBefore = _rss();
  final both = Stopwatch()..start();
  var aleraReadyMicros = -1;
  var codeForgeReadyMicros = -1;

  final aleraFuture = alera_rust.RustLib.init().then((_) {
    aleraReadyMicros = both.elapsedMicroseconds;
  });
  final codeForgeFuture = code_forge_rust.RustLib.init().then((_) {
    codeForgeReadyMicros = both.elapsedMicroseconds;
  });
  await Future.wait<void>(<Future<void>>[aleraFuture, codeForgeFuture]);
  both.stop();
  final rssAfterBothInit = _rss();
  final aleraFirstCallMicros = _firstAleraCall();
  final codeForgeFirstCallMicros = _firstCodeForgeCall();

  return <String, Object?>{
    'aleraReadyMicros': aleraReadyMicros,
    'codeForgeReadyMicros': codeForgeReadyMicros,
    'bothReadyMicros': both.elapsedMicroseconds,
    'aleraFirstCallMicros': aleraFirstCallMicros,
    'codeForgeFirstCallMicros': codeForgeFirstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterBothInitBytes': rssAfterBothInit,
    'rssAfterFirstCallsBytes': _rss(),
  };
}

Future<_Report> _aleraPreopened() async {
  if (_aleraLibraryPath.isEmpty) {
    throw StateError('S1_ALERA_LIBRARY_PATH is required for alera_preopened.');
  }
  final rssBefore = _rss();
  late ExternalLibrary library;
  final libraryOpenMicros = _measureSync(() {
    library = ExternalLibrary.open(_aleraLibraryPath);
  });
  final rssAfterOpen = _rss();
  final frbInitMicros = await _measureAsync(
    () => alera_rust.RustLib.init(externalLibrary: library),
  );
  final rssAfterInit = _rss();
  final firstCallMicros = _firstAleraCall();
  return <String, Object?>{
    'libraryOpenMicros': libraryOpenMicros,
    'frbInitMicros': frbInitMicros,
    'openPlusInitMicros': libraryOpenMicros + frbInitMicros,
    'aleraFirstCallMicros': firstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterOpenBytes': rssAfterOpen,
    'rssAfterInitBytes': rssAfterInit,
    'rssAfterFirstCallBytes': _rss(),
  };
}

Future<_Report> _codeForgePreopened() async {
  if (_codeForgeLibraryPath.isEmpty) {
    throw StateError(
      'S1_CODE_FORGE_LIBRARY_PATH is required for codeforge_preopened.',
    );
  }
  final rssBefore = _rss();
  late ExternalLibrary library;
  final libraryOpenMicros = _measureSync(() {
    library = ExternalLibrary.open(_codeForgeLibraryPath);
  });
  final rssAfterOpen = _rss();
  final frbInitMicros = await _measureAsync(
    () => code_forge_rust.RustLib.init(externalLibrary: library),
  );
  final rssAfterInit = _rss();
  final firstCallMicros = _firstCodeForgeCall();
  return <String, Object?>{
    'libraryOpenMicros': libraryOpenMicros,
    'frbInitMicros': frbInitMicros,
    'openPlusInitMicros': libraryOpenMicros + frbInitMicros,
    'codeForgeFirstCallMicros': firstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterOpenBytes': rssAfterOpen,
    'rssAfterInitBytes': rssAfterInit,
    'rssAfterFirstCallBytes': _rss(),
  };
}

void _requireBothLibraryPaths() {
  if (_aleraLibraryPath.isEmpty || _codeForgeLibraryPath.isEmpty) {
    throw StateError(
      'Both --alera-library and --codeforge-library are required.',
    );
  }
}

Future<_Report> _sequentialPreopened() async {
  _requireBothLibraryPaths();
  final rssBefore = _rss();
  late ExternalLibrary aleraLibrary;
  late ExternalLibrary codeForgeLibrary;
  final total = Stopwatch()..start();
  final aleraOpenMicros = _measureSync(() {
    aleraLibrary = ExternalLibrary.open(_aleraLibraryPath);
  });
  final aleraInitMicros = await _measureAsync(
    () => alera_rust.RustLib.init(externalLibrary: aleraLibrary),
  );
  final codeForgeOpenMicros = _measureSync(() {
    codeForgeLibrary = ExternalLibrary.open(_codeForgeLibraryPath);
  });
  final codeForgeInitMicros = await _measureAsync(
    () => code_forge_rust.RustLib.init(externalLibrary: codeForgeLibrary),
  );
  total.stop();
  final rssAfterBothInit = _rss();
  final aleraFirstCallMicros = _firstAleraCall();
  final codeForgeFirstCallMicros = _firstCodeForgeCall();
  return <String, Object?>{
    'aleraOpenMicros': aleraOpenMicros,
    'aleraInitMicros': aleraInitMicros,
    'codeForgeOpenMicros': codeForgeOpenMicros,
    'codeForgeInitMicros': codeForgeInitMicros,
    'bothReadyMicros': total.elapsedMicroseconds,
    'aleraFirstCallMicros': aleraFirstCallMicros,
    'codeForgeFirstCallMicros': codeForgeFirstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterBothInitBytes': rssAfterBothInit,
    'rssAfterFirstCallsBytes': _rss(),
  };
}

Future<_Report> _concurrentPreopened() async {
  _requireBothLibraryPaths();
  final rssBefore = _rss();
  late ExternalLibrary aleraLibrary;
  late ExternalLibrary codeForgeLibrary;
  final total = Stopwatch()..start();
  final aleraOpenMicros = _measureSync(() {
    aleraLibrary = ExternalLibrary.open(_aleraLibraryPath);
  });
  final codeForgeOpenMicros = _measureSync(() {
    codeForgeLibrary = ExternalLibrary.open(_codeForgeLibraryPath);
  });
  final initWatch = Stopwatch()..start();
  var aleraReadyMicros = -1;
  var codeForgeReadyMicros = -1;
  final aleraFuture = alera_rust.RustLib.init(externalLibrary: aleraLibrary)
      .then((_) {
        aleraReadyMicros = initWatch.elapsedMicroseconds;
      });
  final codeForgeFuture =
      code_forge_rust.RustLib.init(externalLibrary: codeForgeLibrary).then((_) {
        codeForgeReadyMicros = initWatch.elapsedMicroseconds;
      });
  await Future.wait<void>(<Future<void>>[aleraFuture, codeForgeFuture]);
  initWatch.stop();
  total.stop();
  final rssAfterBothInit = _rss();
  final aleraFirstCallMicros = _firstAleraCall();
  final codeForgeFirstCallMicros = _firstCodeForgeCall();
  return <String, Object?>{
    'aleraOpenMicros': aleraOpenMicros,
    'codeForgeOpenMicros': codeForgeOpenMicros,
    'aleraReadyFromInitStartMicros': aleraReadyMicros,
    'codeForgeReadyFromInitStartMicros': codeForgeReadyMicros,
    'concurrentInitMicros': initWatch.elapsedMicroseconds,
    'bothReadyMicros': total.elapsedMicroseconds,
    'aleraFirstCallMicros': aleraFirstCallMicros,
    'codeForgeFirstCallMicros': codeForgeFirstCallMicros,
    'rssBeforeBytes': rssBefore,
    'rssAfterBothInitBytes': rssAfterBothInit,
    'rssAfterFirstCallsBytes': _rss(),
  };
}

Future<_Report> _runScenario() => switch (_scenario) {
  'alera_default' => _aleraDefault(),
  'codeforge_default' => _codeForgeDefault(),
  'sequential_default' => _sequentialDefault(),
  'concurrent_default' => _concurrentDefault(),
  'alera_preopened' => _aleraPreopened(),
  'codeforge_preopened' => _codeForgePreopened(),
  'sequential_preopened' => _sequentialPreopened(),
  'concurrent_preopened' => _concurrentPreopened(),
  _ => throw StateError('Unknown S1_RUST_INIT_SCENARIO: $_scenario'),
};

Future<void> main(List<String> args) async {
  for (final arg in args) {
    if (arg.startsWith('--scenario=')) {
      _scenario = arg.substring('--scenario='.length);
    } else if (arg.startsWith('--alera-library=')) {
      _aleraLibraryPath = arg.substring('--alera-library='.length);
    } else if (arg.startsWith('--codeforge-library=')) {
      _codeForgeLibraryPath = arg.substring('--codeforge-library='.length);
    } else {
      throw ArgumentError('Unknown S1 argument: $arg');
    }
  }

  final report = await _runScenario();
  final envelope = <String, Object?>{
    'scenario': _scenario,
    'pid': pid,
    'platform': Platform.operatingSystem,
    'platformVersion': Platform.operatingSystemVersion,
    ...report,
  };
  print('S1_RUST_COLD_INIT ${jsonEncode(envelope)}');
}
