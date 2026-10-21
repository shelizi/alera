/// Alera isolated Explorer Windows Profile benchmark host-side driver.
library;

import 'package:integration_test/integration_test_driver.dart';

Future<void> main() => integrationDriver(
  timeout: const Duration(minutes: 16),
  writeResponseOnFailure: true,
  responseDataCallback: (data) => writeResponseData(
    data,
    testOutputFilename: 'explorer_windows_profile',
    destinationDirectory: 'build/benchmarks/explorer-profile',
  ),
);
