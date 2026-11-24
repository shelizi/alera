/// Compares the previous full-scan budget formula with the incremental totals
/// on the exact same resident worker-backed terminal. Setup and parsing are
/// deliberately excluded. Run with flutter test --no-pub <this file>.
import 'package:alera/src/features/workbench/presentation/terminal_buffer_budget.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('profile repeated budget checks with 12000 resident rows', () async {
    const rows = 12000;
    const checks = 256;
    final worker = await TerminalXtermWorker.start(
      cols: 160,
      rows: 44,
      maxLines: rows,
    );
    addTearDown(worker.close);
    final replica = TerminalXtermReplicaTerminal(
      cols: 160,
      rows: 44,
      maxLines: rows,
      notificationsEnabled: false,
    );
    addTearDown(replica.dispose);
    final text = List<String>.generate(
      rows,
      (index) => 'log $index ' + 'x' * (index % 100) + ' 界 e\u0301',
    ).join('\r\n');
    replica.applyBufferDelta(await worker.hydrateSnapshotBufferDelta(text));
    final model = replica.replicaModel;
    expect(model.bufferLength, rows);

    int scan() {
      var bytes = measureTerminalCellBufferBytes(replica);
      for (var row = 0; row < model.bufferLength; row++) {
        bytes +=
            model.packedRows[row].lengthInBytes +
            model.rowText(row).length * 2 +
            96;
      }
      return bytes;
    }

    int cached() => replica.retainedCellBufferBytes + model.retainedBytes;

    final expected = scan();
    expect(cached(), expected);

    double measure(int Function() read) {
      var sum = 0;
      final watch = Stopwatch()..start();
      for (var check = 0; check < checks; check++) {
        sum += read();
      }
      watch.stop();
      expect(sum, expected * checks);
      return watch.elapsedTicks * 1000 / watch.frequency;
    }

    for (var warmup = 0; warmup < 3; warmup++) {
      measure(scan);
      measure(cached);
    }
    final scans = <double>[];
    final incremental = <double>[];
    for (var sample = 0; sample < 7; sample++) {
      // Alternate measurement order to reduce order-dependent bias.
      if (sample.isEven) {
        scans.add(measure(scan));
        incremental.add(measure(cached));
      } else {
        incremental.add(measure(cached));
        scans.add(measure(scan));
      }
    }
    scans.sort();
    incremental.sort();
    print('MEMORY_ACCOUNTING rows=$rows checks=$checks bytes=$expected');
    print('FULL_SCAN_MS ' + scans[3].toStringAsFixed(6));
    print('INCREMENTAL_MS ' + incremental[3].toStringAsFixed(6));
    // Timing is diagnostic, not a flaky performance assertion.
  });
}
