"""Controlled Windows Terminal Job memory isolation test.

Runs two sidecar Workers with independent PTYs: only the victim's shell
subtree is capped at 384 MiB. A child inside that PTY tries a single 512 MiB
VirtualAlloc MEM_COMMIT; its failure must not stop the sibling Terminal.

No machine-wide stress, no page touching, no background process leaks.
Usage: python tool/debug/test_terminal_worker_memory_limit.py D:/c/t/debug/alera.exe
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from test_terminal_worker_isolation import Worker

CAP_MB = 384
ATTEMPT_MB = 512


def main(executable):
    if sys.platform != "win32":
        raise SystemExit("Windows only")
    executable = Path(executable).resolve()
    if not executable.is_file():
        raise FileNotFoundError(executable)
    probe = Path(__file__).with_name("pty_job_memory_probe.py").resolve()
    if not probe.is_file():
        raise FileNotFoundError(probe)
    with tempfile.TemporaryDirectory(prefix="alera-pty-memory-", dir=Path.cwd()) as runtime:
        workers = []
        try:
            victim = Worker(
                executable, runtime, "memory-limited",
                worker_env={"ALERA_TERMINAL_JOB_MEMORY_LIMIT_MB": str(CAP_MB)}
            )
            workers.append(victim)
            sibling = Worker(
                executable, runtime, "unlimited-sibling",
                worker_env={"ALERA_TERMINAL_JOB_MEMORY_LIMIT_MB": "0"}
            )
            workers.append(sibling)

            victim.assert_echo("ALERA_LIMITED_READY", "ALERA_LIMITED_READY")
            sibling.assert_echo("ALERA_SIBLING_READY", "ALERA_SIBLING_READY")

            # Child only asks the OS to commit memory, never writes 512 MiB
            # of pages. Its stdout marker is in the script, not in the command
            # or terminal echo, so it cannot produce a false positive.
            command = subprocess.list2cmdline(
                [sys.executable, str(probe), str(ATTEMPT_MB)]
            )
            victim.send_bytes((command + "\r").encode())
            sibling.assert_echo(
                "ALERA_SIBLING_DURING_MEMORY",
                "ALERA_SIBLING_DURING_MEMORY",
            )
            victim.read_until(
                lambda data: (
                    b"ALERA_MEM_ALLOCATION_REJECTED:" in data
                    or b"ALERA_MEM_ALLOCATION_SUCCEEDED" in data
                ),
                timeout=25,
            )
            output = bytes(victim.data)
            assert b"ALERA_MEM_ALLOCATION_REJECTED:" in output, (
                "Windows failed to enforce 384 MiB shell Job memory limit; "
                f"last_output={output[-1600:]!r}"
            )
            assert b"ALERA_MEM_ALLOCATION_SUCCEEDED" not in output
            assert victim.process.poll() is None, (
                "shell Worker died instead of rejecting the child allocation"
            )
            victim.assert_echo("ALERA_VICTIM_AFTER_MEMORY", "ALERA_VICTIM_AFTER_MEMORY")
            sibling.assert_echo("ALERA_SIBLING_AFTER_MEMORY", "ALERA_SIBLING_AFTER_MEMORY")
            print(
                f"PASS: {CAP_MB} MiB shell Job rejected {ATTEMPT_MB} MiB child commit; "
                "victim and independent sibling terminals remain responsive"
            )
        finally:
            for worker in reversed(workers):
                worker.close()


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: test_terminal_worker_memory_limit.py PATH_TO_ALERA_EXE"
        )
    main(sys.argv[1])
