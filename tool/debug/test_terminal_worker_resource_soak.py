"""Bounded 60-second Windows PTY Worker memory/CPU/reconnect soak.

Creates two real isolated PTY Workers under a disposable runtime. Repeats
authenticated Host-socket disconnect/reconnect and input to both workers.
Samples RSS and process CPU to detect clear leaks. No physical OOM, no
unbounded output or infinite background processes.

Usage:
    python tool/debug/test_terminal_worker_resource_soak.py D:/c/t/debug/alera.exe
"""
from pathlib import Path
import os
import sys
import tempfile
import time

import psutil
from test_terminal_worker_isolation import Worker

DURATION_SECONDS = int(os.environ.get("ALERA_WORKER_SOAK_SECONDS", "60"))
SAMPLE_INTERVAL_SECONDS = 2
RECONNECT_EVERY_SECONDS = 6
MAX_WORKER_RSS_INCREASE_BYTES = 64 * 1024 * 1024
MAX_WORKER_RSS_ABSOLUTE_BYTES = 256 * 1024 * 1024


def check_running(worker, process):
    assert worker.process.poll() is None, f"{worker.label} worker exited during soak"
    assert process.is_running(), f"{worker.label} operating system process disappeared"


def run_confirmed_command(worker, marker_path, marker):
    # CMD echoes keystrokes before executing a command. Checking the PTY
    # stream for the same text can incorrectly count queued input as success.
    # A marker file proves the shell processed Enter and finished execution.
    marker_path.unlink(missing_ok=True)
    worker.send_bytes(f'echo {marker} > "{marker_path}"\r'.encode("utf-8"))
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if worker.process.poll() is not None:
            raise AssertionError(f"{worker.label}: PTY worker exited")
        try:
            if marker_path.read_text(encoding="utf-8").strip() == marker:
                return
        except (FileNotFoundError, PermissionError):
            pass
        time.sleep(0.05)
    raise TimeoutError(
        f"{worker.label}: shell never completed marker {marker}; "
        f"PTY tail={bytes(worker.data[-1200:])!r}"
    )


def main(executable):
    if sys.platform != "win32":
        raise SystemExit("Windows-only resource soak")
    executable = Path(executable).resolve()
    if not executable.is_file():
        raise FileNotFoundError(executable)

    with tempfile.TemporaryDirectory(prefix="alera-worker-soak-", dir=Path.cwd()) as runtime:
        workers = []
        try:
            primary = Worker(executable, runtime, "soak-primary", max_backlog_bytes=65536)
            workers.append(primary)
            sibling = Worker(executable, runtime, "soak-sibling", max_backlog_bytes=65536)
            workers.append(sibling)
            primary.assert_echo("ALERA_PRIMARY_WARM", "ALERA_PRIMARY_WARM")
            sibling.assert_echo("ALERA_SIBLING_WARM", "ALERA_SIBLING_WARM")

            processes = [psutil.Process(worker.process.pid) for worker in workers]
            identities = [(process.pid, process.create_time()) for process in processes]
            for process in processes:
                process.cpu_percent(None)
            baseline_rss = [process.memory_info().rss for process in processes]
            max_rss = baseline_rss[:]
            cpu_start = [
                sum(process.cpu_times()[:2]) for process in processes
            ]
            started = time.monotonic()
            last_sample = started
            last_reconnect = started
            primary_marker = Path(runtime) / "primary-complete.txt"
            sibling_marker = Path(runtime) / "sibling-complete.txt"
            rounds = 0
            reconnects = 0
            while time.monotonic() - started < DURATION_SECONDS:
                rounds += 1
                run_confirmed_command(
                    primary, primary_marker, f"ALERA_PRIMARY_{rounds}"
                )
                run_confirmed_command(
                    sibling, sibling_marker, f"ALERA_SIBLING_{rounds}"
                )

                now = time.monotonic()
                if now - last_reconnect >= RECONNECT_EVERY_SECONDS:
                    primary.disconnect()
                    assert primary.process.poll() is None, "Worker died during Host disconnect"
                    primary.attach()
                    assert primary.process.pid == identities[0][0]
                    run_confirmed_command(
                        primary, primary_marker, f"ALERA_RECOVERED_{reconnects}"
                    )
                    reconnects += 1
                    last_reconnect = time.monotonic()

                if now - last_sample >= SAMPLE_INTERVAL_SECONDS:
                    for i, process in enumerate(processes):
                        check_running(workers[i], process)
                        assert process.create_time() == identities[i][1], (
                            "PID was replaced during worker soak"
                        )
                        rss = process.memory_info().rss
                        max_rss[i] = max(max_rss[i], rss)
                        assert rss < MAX_WORKER_RSS_ABSOLUTE_BYTES, (
                            f"Worker {i} RSS exceeded 256 MiB: {rss}"
                        )
                        assert rss <= baseline_rss[i] + MAX_WORKER_RSS_INCREASE_BYTES, (
                            f"Worker {i} RSS increased >64 MiB: baseline="
                            f"{baseline_rss[i]}, observed={rss}"
                        )
                    last_sample = time.monotonic()
                time.sleep(0.4)

            elapsed = time.monotonic() - started
            final_rss = [process.memory_info().rss for process in processes]
            cpu_seconds = [
                sum(process.cpu_times()[:2]) - cpu_start[i]
                for i, process in enumerate(processes)
            ]
            for i in range(2):
                assert final_rss[i] < MAX_WORKER_RSS_ABSOLUTE_BYTES
                assert max_rss[i] <= baseline_rss[i] + MAX_WORKER_RSS_INCREASE_BYTES
            print(
                f"PASS: {elapsed:.1f}s, {rounds} paired inputs, "
                f"{reconnects} authenticated recoveries; "
                f"RSS baseline/peak/final KiB="
                f"{[(a//1024,b//1024,c//1024) for a,b,c in zip(baseline_rss,max_rss,final_rss)]}; "
                f"worker CPU seconds={[round(v,2) for v in cpu_seconds]}"
            )
        finally:
            for worker in reversed(workers):
                worker.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_terminal_worker_resource_soak.py PATH_TO_ALERA_EXE")
    main(sys.argv[1])
