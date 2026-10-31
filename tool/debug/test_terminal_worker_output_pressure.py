"""Bounded offline-output and sibling-isolation smoke for Windows PTY workers.

Runs only under an isolated runtime. Generates 256 KiB of ConPTY output
while the initial Host socket is disconnected, with a 64 KiB tail limit.
Reconnection must report an advanced absolute replay cursor and deliver the
final output marker. A separate worker remains responsive throughout.
"""
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from test_terminal_worker_isolation import Worker

OUTPUT_BYTES = 256 * 1024
TAIL_BYTES = 64 * 1024
END_MARKER = b"ALERA_OFFLINE_REPLAY_END"


def main(executable):
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    exe = Path(executable).resolve()
    if not exe.is_file():
        raise FileNotFoundError(exe)
    with tempfile.TemporaryDirectory(prefix="alera-output-pressure-", dir=Path.cwd()) as runtime:
        workers = []
        try:
            busy = Worker(exe, runtime, "offline-output", max_backlog_bytes=TAIL_BYTES)
            workers.append(busy)
            sibling = Worker(exe, runtime, "output-sibling")
            workers.append(sibling)
            busy.assert_echo("ALERA_BUSY_READY", "ALERA_BUSY_READY")
            sibling.assert_echo("ALERA_SIBLING_READY", "ALERA_SIBLING_READY")

            old_cursor = busy.replay_start_cursor
            snippet = (
                f"import sys;sys.stdout.write('X'*{OUTPUT_BYTES});"
                "print();print('ALERA'+'_OFFLINE_REPLAY_END',flush=True)"
            )
            command = subprocess.list2cmdline([sys.executable, "-c", snippet])
            busy.send_bytes((command + "\r").encode("utf-8"))
            busy.disconnect()
            # The worker still owns ConPTY while no host is reading its output.
            time.sleep(7)
            assert busy.process.poll() is None, "output load crashed the isolated PTY worker"
            sibling.assert_echo(
                "ALERA_SIBLING_DURING_OUTPUT", "ALERA_SIBLING_DURING_OUTPUT"
            )
            busy.attach()
            busy.read_until(lambda output: END_MARKER in output, timeout=25)
            assert busy.replay_start_cursor > old_cursor, (
                "offline output did not advance the bounded replay cursor"
            )
            assert busy.replay_start_cursor > TAIL_BYTES, (
                f"offline output was not trimmed: cursor={busy.replay_start_cursor}"
            )
            assert len(busy.data) < OUTPUT_BYTES, (
                "Worker replayed unbounded output instead of keeping its tail"
            )
            busy.assert_echo("ALERA_BUSY_AFTER_OUTPUT", "ALERA_BUSY_AFTER_OUTPUT")
            sibling.assert_echo("ALERA_SIBLING_AFTER_OUTPUT", "ALERA_SIBLING_AFTER_OUTPUT")
            print(
                f"PASS: offline 256 KiB output truncated to 64 KiB tail "
                f"(replay start {busy.replay_start_cursor}); "
                "both PTY workers remained responsive"
            )
        finally:
            for worker in reversed(workers):
                worker.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(
            "Usage: test_terminal_worker_output_pressure.py PATH_TO_ALERA_EXE"
        )
    main(sys.argv[1])
