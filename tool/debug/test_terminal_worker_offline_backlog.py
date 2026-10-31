"""Bounded offline PTY output replay test: 6 MiB output, 64 KiB ring.

A Worker owns the PTY after its simulated Host disconnects. Once the child
finishes writing, a new authenticated connection must replay only the
bounded tail, not 6 MiB, while preserving the same Shell and Worker PIDs.

Usage: python tool/debug/test_terminal_worker_offline_backlog.py D:/c/t/debug/alera.exe
"""
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from test_terminal_worker_isolation import Worker

OUTPUT_BYTES = 6 * 1024 * 1024
BACKLOG_BYTES = 64 * 1024


def main(executable):
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    executable = Path(executable).resolve()
    if not executable.is_file():
        raise FileNotFoundError(executable)
    probe = Path(__file__).with_name("pty_output_flood_probe.py").resolve()
    with tempfile.TemporaryDirectory(prefix="alera-pty-output-", dir=Path.cwd()) as runtime:
        done = Path(runtime) / "flood.done"
        worker = None
        try:
            worker = Worker(
                executable, runtime, "offline-flood",
                max_backlog_bytes=BACKLOG_BYTES,
            )
            worker.assert_echo("ALERA_FLOOD_START", "ALERA_FLOOD_START")
            original_worker_pid = worker.process.pid
            original_shell_pid = worker.shell_pid

            command = subprocess.list2cmdline([sys.executable, str(probe), str(done)])
            worker.send_bytes((command + "\r").encode("utf-8"))
            worker.disconnect()

            deadline = time.monotonic() + 35
            while not done.is_file() and time.monotonic() < deadline:
                if worker.process.poll() is not None:
                    raise AssertionError("PTY Worker unexpectedly exited during flood")
                time.sleep(0.1)
            if not done.is_file():
                raise TimeoutError("bounded PTY output flood did not complete")
            # Ensure ConPTY drain catches up with the child exit before
            # reconnecting. Worker output ring stays at max 64 KiB.
            time.sleep(1.0)
            worker.attach()
            worker.read_until(
                lambda bytes_: b"ALERA_OFFLINE_FLOOD_DONE" in bytes_,
                timeout=15,
            )
            assert worker.process.pid == original_worker_pid
            assert worker.shell_pid == original_shell_pid
            assert worker.replay_start_cursor > OUTPUT_BYTES - BACKLOG_BYTES * 2, (
                "PTY output ring did not advance as bytes were discarded; "
                f"cursor={worker.replay_start_cursor}"
            )
            # The replay may include a few terminal cursor/control frames,
            # but can never replay the multi-MB offline flood.
            assert len(worker.data) <= BACKLOG_BYTES * 3, (
                f"offline replay exceeded bounded ring: {len(worker.data)} bytes"
            )
            worker.assert_echo(
                "ALERA_FLOOD_SURVIVED", "ALERA_FLOOD_SURVIVED"
            )
            print(
                f"PASS: {OUTPUT_BYTES} output bytes with {BACKLOG_BYTES}B offline ring; "
                f"replayed={len(worker.data)}B, absolute_cursor={worker.replay_start_cursor}; "
                "original PTY stayed responsive"
            )
        finally:
            if worker is not None:
                worker.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: test_terminal_worker_offline_backlog.py PATH_TO_ALERA_EXE"
        )
    main(sys.argv[1])
