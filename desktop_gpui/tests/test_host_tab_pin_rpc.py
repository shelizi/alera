"""Opt-in Windows end-to-end Host pin CAS smoke, using only disposable state.

Run with:
  python desktop_gpui/tests/test_host_tab_pin_rpc.py --cli D:/path/to/branch/alera.exe

Requires a CLI built from this branch. Never opens a production Runtime Host,
starts an Agent or PTY, or reads the user's actual Alera profile. The direct
authenticated RPC connection emulates Classic's Host client; the other client
uses the same --no-start CLI bridge as GPUI. This is NOT a visual GUI test.
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import tempfile
import time


def assert_true(condition, message):
    if not condition:
        raise AssertionError(message)


def call_cli(cli, runtime, env, *args, ok=True):
    result = subprocess.run(
        [str(cli), *args],
        env=env,
        cwd=str(runtime.parent),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=20,
        check=False,
    )
    if ok:
        assert_true(
            result.returncode == 0,
            f"isolated CLI command failed with exit={result.returncode}: "
            f"{result.stderr[:500]}",
        )
        return json.loads(result.stdout)
    assert_true(result.returncode != 0, "stale/invalid Host mutation was accepted")
    return result


def settings(cli, runtime, env, verb, *args, ok=True):
    return call_cli(
        cli, runtime, env, "settings", "--no-start", "--json",
        "--runtime-dir", str(runtime), verb, *args, ok=ok,
    )


def assert_projection(value):
    assert_true(
        isinstance(value, dict)
        and set(value) == {
            "id", "workspaceId", "updatedAt", "pinned", "terminalSessionId"
        },
        "Host pin CLI returned a full Tab or an unexpected projection",
    )
    assert_true("initialCommand" not in json.dumps(value), "Agent command was exposed")


class DisposableHost:
    def __init__(self, cli, runtime, env, token):
        self.cli, self.runtime, self.env, self.token = cli, runtime, env, token
        self.control = runtime / "runtime-host.json"
        self.process = None
        self.port = None

    def start(self):
        assert_true(self.process is None, "test Host already running")
        self.process = subprocess.Popen(
            [
                str(self.cli), "runtime-host", "--runtime-dir", str(self.runtime),
                "--control-file", str(self.control), "--token", self.token,
                "--empty-shutdown-delay-seconds", "120",
                "--detached-session-shutdown-delay-seconds", "120",
            ],
            env=self.env,
            cwd=str(self.runtime.parent),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        end = time.monotonic() + 30
        while time.monotonic() < end:
            assert_true(self.process.poll() is None, "isolated Host exited during startup")
            try:
                control = json.loads(self.control.read_text(encoding="utf-8"))
                if control.get("pid") == self.process.pid and isinstance(control.get("port"), int):
                    self.port = control["port"]
                    return
            except (FileNotFoundError, ValueError, OSError):
                pass
            time.sleep(0.05)
        raise AssertionError("isolated Host did not publish its own control file")

    def stop(self):
        if self.process is not None:
            self.process.terminate()
            try:
                self.process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
            self.process = None
            self.port = None


class ClassicHostClient:
    def __init__(self, port, token):
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=12)
        self.socket.settimeout(12)
        self.reader = self.socket.makefile("rb")
        self.sequence = 0
        self.send("hello", {"protocolVersion": 4, "token": token})

    def send(self, kind, payload):
        self.sequence += 1
        number = self.sequence
        frame = json.dumps(
            {"id": number, "type": kind, "payload": payload}, ensure_ascii=False
        ).encode("utf-8") + b"\n"
        self.socket.sendall(frame)
        for _ in range(100):
            line = self.reader.readline()
            assert_true(line, "isolated Host disconnected")
            received = json.loads(line)
            if received.get("id") == number:
                assert_true(
                    received.get("ok") is True,
                    f"Classic Host RPC {kind} failed: {str(received.get('error'))[:450]}",
                )
                return received.get("payload")
        raise AssertionError("too many unrelated Host events")

    def close(self):
        self.reader.close()
        self.socket.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    args = parser.parse_args()
    cli = args.cli.resolve(strict=True)
    assert_true(cli.is_absolute() and cli.is_file(), "branch CLI must exist")
    assert_true(os.name == "nt", "live Host test currently targets Windows")

    checks = []
    with tempfile.TemporaryDirectory(prefix="alera-pin-cas-") as scratch:
        root = Path(scratch)
        runtime = root / "runtime"
        home = root / "home"
        runtime.mkdir()
        home.mkdir()
        # Never pass user/session-specific Alera paths, tokens, or unrelated
        # account credentials into the disposable Host fixture.
        allowed_env = {"PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TEMP", "TMP"}
        env = {key: value for key, value in os.environ.items()
               if key.upper() in allowed_env}
        env.update({
            "HOME": str(home),
            "USERPROFILE": str(home),
            "APPDATA": str(home / "AppData" / "Roaming"),
            "LOCALAPPDATA": str(home / "AppData" / "Local"),
            "ALERA_TERMINAL_WORKER_PROCESS": "0",
        })
        for name in ("APPDATA", "LOCALAPPDATA"):
            Path(env[name]).mkdir(parents=True)
        workspace_id = "gpui-pin-isolated-workspace"
        tab_id = "gpui-pin-isolated-tab"
        session_id = "gpui-pin-isolated-session"
        secret = "NEVER_EXPORT_AGENT_INSTRUCTION"
        token = secrets.token_hex(32)
        host = DisposableHost(cli, runtime, env, token)
        try:
            settings(cli, runtime, env, "pin-list", "--workspace-id", workspace_id, ok=False)
            assert_true(not host.control.exists(), "--no-start unexpectedly started Host")
            checks.append("no_start_no_production_host")
            host.start()
            call_cli(
                cli, runtime, env, "project", "--runtime-dir", str(runtime),
                "--json", "add", "--id", "gpui-pin-project", "--name",
                "Disposable pin project", "--repo-path", str(home), "--kind", "folder",
            )
            call_cli(
                cli, runtime, env, "workspace", "--runtime-dir", str(runtime),
                "--json", "register", "--id", workspace_id,
                "--project-id", "gpui-pin-project", "--name", "Disposable pin workspace",
                "--path", str(home), "--kind", "main",
            )
            classic = ClassicHostClient(host.port, token)
            try:
                stamp = datetime.datetime.now(
                    datetime.timezone.utc
                ).isoformat(timespec="milliseconds").replace("+00:00", "Z")
                original = {
                    "id": tab_id,
                    "workspaceId": workspace_id,
                    "kind": "terminal",
                    "title": "Agent pin fixture",
                    "createdAt": stamp,
                    "updatedAt": stamp,
                    "payload": {
                        "terminalSessionId": session_id,
                        "pinned": False,
                        "preview": True,
                        "initialCommand": secret,
                        "agentTitleRevision": "private-agent-revision",
                    },
                }
                classic.send("tab.upsert", original)
                first = settings(cli, runtime, env, "pin-list", "--workspace-id", workspace_id)
                assert_true(len(first) == 1, "Native projection lost Host tab")
                observed = first[0]
                assert_projection(observed)
                assert_true(observed["pinned"] is False, "new tab incorrectly pinned")
                assert_true(observed["terminalSessionId"] == session_id, "wrong Host PTY identity")
                checks.append("classic_create_native_read")

                def native_cas(snapshot, pinned, expected_session=session_id, ok=True):
                    return settings(
                        cli, runtime, env, "pin-set",
                        "--workspace-id", workspace_id,
                        "--id", tab_id,
                        "--expected-updated-at", snapshot["updatedAt"],
                        "--expected-pinned", str(snapshot["pinned"]).lower(),
                        "--pinned", str(pinned).lower(),
                        "--expected-terminal-session-id", expected_session,
                        ok=ok,
                    )

                pinned = native_cas(observed, True)
                assert_projection(pinned)
                assert_true(pinned["pinned"] is True, "Native CLI pin CAS was not persisted")
                assert_true(pinned["updatedAt"] != observed["updatedAt"], "revision did not advance")
                stored = classic.send("tab.find", {"id": tab_id})
                assert_true(stored["payload"]["pinned"] is True, "Classic saw wrong pin")
                assert_true("preview" not in stored["payload"], "pinned preview flag survived")
                assert_true(stored["payload"]["initialCommand"] == secret, "Agent metadata changed")
                checks.append("native_pin_classic_readback_no_secret")

                native_cas(observed, False, ok=False)
                native_cas(pinned, False, expected_session="recycled-pty", ok=False)
                assert_true(
                    settings(cli, runtime, env, "pin-list", "--workspace-id", workspace_id)[0]["pinned"]
                    is True, "stale CAS or recycled PTY mutated pin",
                )
                checks.append("stale_revision_and_recycled_session_denied")

                # Simulate Classic replaying an older tab.upsert: Host must
                # retain its independently owned current pin and protect the
                # sensitive Agent instruction from a Native projection.
                classic.send("tab.upsert", original)
                after_replay = settings(
                    cli, runtime, env, "pin-list", "--workspace-id", workspace_id
                )[0]
                assert_true(after_replay["pinned"] is True, "stale Classic upsert overwrote pin")
                assert_projection(after_replay)
                checks.append("classic_stale_upsert_preserves_host_pin")

                # The Native mixed strip may attach Host tabs from multiple
                # registered Workspaces. CLI pin metadata must remain scoped
                # to the exact owner rather than leaking into the root.
                attached_root = home / "attached-root"
                attached_root.mkdir()
                attached_workspace = "gpui-pin-attached-workspace"
                call_cli(
                    cli, runtime, env, "project", "--runtime-dir", str(runtime),
                    "--json", "add", "--id", "gpui-pin-attached-project",
                    "--name", "Attached pin project",
                    "--repo-path", str(attached_root), "--kind", "folder",
                )
                call_cli(
                    cli, runtime, env, "workspace", "--runtime-dir", str(runtime),
                    "--json", "register", "--id", attached_workspace,
                    "--project-id", "gpui-pin-attached-project",
                    "--name", "Attached pin workspace", "--path",
                    str(attached_root), "--kind", "main",
                )
                attached_tab = {
                    **original,
                    "id": "gpui-pin-attached-tab",
                    "workspaceId": attached_workspace,
                    "payload": {
                        **original["payload"],
                        "terminalSessionId": "gpui-pin-attached-pty",
                    },
                }
                classic.send("tab.upsert", attached_tab)
                attached_observed = settings(
                    cli, runtime, env, "pin-list",
                    "--workspace-id", attached_workspace,
                )
                assert_true(len(attached_observed) == 1, "attached owner Tab was not listed")
                assert_projection(attached_observed[0])
                attached_pinned = settings(
                    cli, runtime, env, "pin-set",
                    "--workspace-id", attached_workspace,
                    "--id", attached_tab["id"],
                    "--expected-updated-at", attached_observed[0]["updatedAt"],
                    "--expected-pinned", "false", "--pinned", "true",
                    "--expected-terminal-session-id", "gpui-pin-attached-pty",
                )
                assert_projection(attached_pinned)
                assert_true(attached_pinned["pinned"] is True, "attached owner CAS failed")
                assert_true(
                    settings(cli, runtime, env, "pin-list",
                             "--workspace-id", workspace_id)[0]["id"] == tab_id,
                    "another workspace polluted the root pin list",
                )
                checks.append("registered_other_workspace_pin_is_scoped")

                classic.send("tab.pin.cas", {
                    "workspaceId": workspace_id,
                    "id": tab_id,
                    "expectedUpdatedAt": after_replay["updatedAt"],
                    "expectedPinned": True,
                    "pinned": False,
                    "expectedTerminalSessionId": session_id,
                })
                unpinned = settings(
                    cli, runtime, env, "pin-list", "--workspace-id", workspace_id
                )[0]
                assert_true(unpinned["pinned"] is False, "Native did not see Classic unpin")
                checks.append("classic_unpin_native_readback")

                repinned = native_cas(unpinned, True)
                assert_true(repinned["pinned"] is True, "re-pin failed")
            finally:
                classic.close()

            host.stop()
            host.start()
            restarted = settings(
                cli, runtime, env, "pin-list", "--workspace-id", workspace_id
            )
            assert_true(len(restarted) == 1, "Host restart lost Tab record")
            assert_projection(restarted[0])
            assert_true(restarted[0]["pinned"] is True, "Host restart lost persistent pin")
            assert_true(
                restarted[0]["terminalSessionId"] == session_id,
                "Host restart changed tab identity",
            )
            remote_restarted = settings(
                cli, runtime, env, "pin-list",
                "--workspace-id", "gpui-pin-attached-workspace",
            )
            assert_true(
                len(remote_restarted) == 1 and remote_restarted[0]["pinned"] is True,
                "Host restart lost another Workspace's pin",
            )
            native_cas(unpinned, False, ok=False)
            checks.append("host_restart_preserves_pin_and_cas_revision")
        finally:
            host.stop()
    print(json.dumps({
        "status": "PASS",
        "checks": checks,
        "passed": len(checks),
        "mode": "disposable real Runtime Host + Classic-equivalent RPC + GPUI CLI; no UI/PTY",
    }, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"DISPOSABLE_HOST_PIN_ACCEPTANCE_FAILED: {exc}", file=sys.stderr)
        raise
