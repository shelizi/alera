#!/usr/bin/env python3
"""Isolated real Host -> CLI stream -> PTY smoke, never the user's live Host.

Usage: python desktop_gpui/tests/terminal_stream_isolated_smoke.py PATH_TO_NEW_ALERA_EXE
       [--native-app PATH_TO_GPUI_EXE --hold-seconds 40]

The test creates one temporary Runtime profile, project, workspace, and PTY,
asserts that GPUI's narrow stream protocol works with a real Host, and shuts
down only the processes that this test started.  No shared Alera profile or
application window is accessed.
"""

import base64
import argparse
import json
import os
from collections import deque
from pathlib import Path
import queue
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid


class SmokeError(Exception):
    pass


def checked_cli(executable, profile, *args, timeout=20):
    result = subprocess.run(
        [str(executable), *args[:1], "--runtime-dir", str(profile), "--json", *args[1:]],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        env={**os.environ, "ALERA_RUNTIME_DIR": str(profile)},
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    if result.returncode != 0:
        # CLI registration has no user terminal data. Keep diagnostics short
        # and never print the Host token or a PTY data frame.
        detail = (result.stderr or "no diagnostic").strip()[-400:]
        raise SmokeError(
            f"CLI {args[0]} {args[1]} failed with code {result.returncode}: {detail}"
        )
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SmokeError(f"CLI {args[0]} {args[1]} returned malformed JSON") from error


class StreamClient:
    def __init__(self, executable, profile):
        self.process = subprocess.Popen(
            [
                str(executable),
                "terminal",
                "--runtime-dir",
                str(profile),
                "--json",
                "--no-start",
                "stream",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
            env={**os.environ, "ALERA_RUNTIME_DIR": str(profile)},
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        self.queue = queue.Queue()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.process.stdout:
            try:
                self.queue.put(json.loads(line))
            except json.JSONDecodeError:
                self.queue.put({"_invalid_json": True})
        self.queue.put({"_eof": True})

    def send(self, request_id, verb, payload):
        if self.process.poll() is not None:
            raise SmokeError("Native stream CLI exited before writing request")
        self.process.stdin.write(
            json.dumps({"id": request_id, "type": verb, "payload": payload}) + "\n"
        )
        self.process.stdin.flush()

    def until(self, predicate, timeout=12, ignored=None):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                item = self.queue.get(timeout=min(0.25, deadline - time.monotonic()))
            except queue.Empty:
                continue
            if item.get("_eof") or item.get("_invalid_json"):
                raise SmokeError("Native stream IPC closed or emitted malformed JSON")
            if predicate(item):
                return item
            if ignored is not None:
                ignored.append(item)
        raise SmokeError("Timed out waiting for Native stream response/output")

    def close(self):
        if self.process.poll() is None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def write_and_expect_host_output(stream, request_id, identity, command, marker, timeout=25):
    """Verify real scoped Host ACK and pushed output, never matching another PTY."""
    stream.send(
        request_id,
        "write",
        {**identity, "dataBase64": base64.b64encode(command.encode("utf-8")).decode()},
    )
    output = bytearray()
    acknowledged = False
    deadline = time.monotonic() + timeout
    expected = marker.encode("utf-8")
    while time.monotonic() < deadline:
        event = stream.until(lambda _: True, timeout=max(0.1, deadline - time.monotonic()))
        if event.get("id") == request_id:
            if not event.get("ok"):
                raise SmokeError("Host rejected scoped PTY input")
            acknowledged = True
        if event.get("event") in ("outputResyncRequired", "exit", "error") and (
            event.get("payload", {}).get("sessionId") == identity["sessionId"]
        ):
            raise SmokeError("Observed PTY exited or requested output resynchronization")
        if event.get("event") == "output":
            payload = event.get("payload", {})
            if payload.get("sessionId") == identity["sessionId"]:
                output.extend(base64.b64decode(payload["dataBase64"], validate=True))
        if acknowledged and expected in output:
            return len(output)
    raise SmokeError("Native stream did not deliver scoped Host output before deadline")


def host_rpc(control, verb, payload):
    # The control file exists *only* inside the random temporary profile.
    with socket.create_connection(("127.0.0.1", int(control["port"])), timeout=3) as client:
        client.settimeout(5)
        reader = client.makefile("rb")
        hello = {
            "id": 1,
            "type": "hello",
            "payload": {
                "protocolVersion": control["protocolVersion"],
                "token": control["token"],
                "clientKind": "cli",
            },
        }
        client.sendall((json.dumps(hello) + "\n").encode())
        if not json.loads(reader.readline()).get("ok"):
            raise SmokeError("Temporary Host refused its own hello")
        client.sendall(
            (json.dumps({"id": 2, "type": verb, "payload": payload}) + "\n").encode()
        )
        for _ in range(10):
            line = reader.readline()
            if not line:
                raise SmokeError("Temporary Host closed its control request")
            result = json.loads(line)
            if result.get("id") == 2:
                return result
    raise SmokeError("Temporary Host did not answer control request")


def wait_host_control(path, process, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise SmokeError(f"Isolated Host exited during startup: {process.returncode}")
        try:
            control = json.loads(path.read_text(encoding="utf-8"))
            if control.get("port") and control.get("token"):
                return control
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    raise SmokeError("Isolated Host did not publish a control file")


def prepare_native_preview_state(state_dir, root, session_id, workspace_id, tab_id,
                                 second_session_id=None, second_tab_id=None):
    """Seed isolated Host previews without recreating Shells or Host identities."""
    state_dir.mkdir(parents=True, exist_ok=True)
    root = root.resolve()
    hash64 = 0xCBF29CE484222325
    for byte in str(root).encode("utf-8"):
        hash64 ^= byte
        hash64 = (hash64 * 0x100000001B3) & ((1 << 64) - 1)
    tabs = [{
        "handle": session_id, "workspace_id": workspace_id, "tab_id": tab_id,
    }]
    if second_session_id is not None:
        if not second_tab_id or second_session_id == session_id:
            raise SmokeError("Invalid independent secondary Native test tab")
        tabs.append({
            "handle": second_session_id, "workspace_id": workspace_id,
            "tab_id": second_tab_id,
        })
    saved = {
        "version": 1,
        "root": str(root),
        "layout": {
            "tabs": tabs,
            "active_handle": session_id,
            "transcript_view": False,
        },
    }
    path = state_dir / f"terminal-layout-{hash64:016x}.json"
    path.write_text(json.dumps(saved, ensure_ascii=False), encoding="utf-8")
    return path


def interrupt_only_native_private_stream(native_process, executable, profile,
                                         timeout=18):
    """Kill only this test Native window's scoped CLI stream child, not the Host.

    This tests Native's automatic 1/2/4/... retry without touching any other
    Alera/Classic process. Both the executable path and temporary runtime
    argument must match, and the child must descend from this Native PID.
    """
    import psutil  # Optional dependency; only required by --native-reconnect.

    executable = os.path.normcase(str(executable.resolve()))
    profile = os.path.normcase(str(profile.resolve()))
    owner = psutil.Process(native_process.pid)

    def find_scoped_streams():
        matches = []
        for child in owner.children(recursive=True):
            try:
                cmdline = child.cmdline()
                if (os.path.normcase(child.exe()) == executable
                        and "terminal" in cmdline and "stream" in cmdline
                        and "--no-start" in cmdline
                        and any(os.path.normcase(arg) == profile for arg in cmdline)):
                    matches.append(child)
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                continue
        return matches

    deadline = time.monotonic() + timeout
    original = None
    while time.monotonic() < deadline:
        matches = find_scoped_streams()
        if len(matches) == 1:
            original = matches[0]
            break
        if native_process.poll() is not None:
            raise SmokeError("Native GUI exited before scoped stream appeared")
        time.sleep(0.2)
    if original is None:
        raise SmokeError("Native never started exactly one private CLI stream")

    first_pid = original.pid
    original.kill()
    original.wait(timeout=5)
    while time.monotonic() < deadline:
        matches = [child for child in find_scoped_streams()
                   if child.pid != first_pid]
        if len(matches) == 1:
            return first_pid, matches[0].pid
        if native_process.poll() is not None:
            raise SmokeError("Native exited after private stream interruption")
        time.sleep(0.2)
    raise SmokeError("Native did not respawn the private Host stream after interruption")


def run_smoke(executable, native_app=None, hold_seconds=25,
              native_reconnect=False):
    run_marker = uuid.uuid4().hex[:14]
    checks = []
    with tempfile.TemporaryDirectory(prefix="alera_gpui_stream_isolated_") as temp:
        root = Path(temp)
        profile = root / "runtime"
        workspace = root / "workspace"
        profile.mkdir()
        workspace.mkdir()
        control_file = profile / "runtime-host.json"
        token = uuid.uuid4().hex + uuid.uuid4().hex
        host = subprocess.Popen(
            [
                str(executable),
                "runtime-host",
                "--runtime-dir",
                str(profile),
                "--control-file",
                str(control_file),
                "--token",
                token,
                "--persistent",
                "--login-shell",
                "false",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL,
            env={**os.environ, "ALERA_RUNTIME_DIR": str(profile)},
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        # Never leave a Windows stderr file locked by an inherited PTY worker.
        # Keep only bounded, token-redacted in-memory diagnostics on failure.
        host_diagnostics = deque(maxlen=24)
        def collect_host_stderr():
            for line in iter(host.stderr.readline, b""):
                host_diagnostics.append(line.decode("utf-8", errors="replace"))
        threading.Thread(target=collect_host_stderr, daemon=True).start()
        session_id = None
        second_session_id = None
        stream = None
        control = None
        native_process = None
        try:
            control = wait_host_control(control_file, host)
            project_id = "project-" + run_marker
            workspace_id = "workspace-" + run_marker
            checked_cli(
                executable,
                profile,
                "project",
                "add",
                "--id",
                project_id,
                "--name",
                "Isolated stream smoke",
                "--repo-path",
                str(workspace),
                "--kind",
                "folder",
            )
            checked_cli(
                executable,
                profile,
                "workspace",
                "register",
                "--id",
                workspace_id,
                "--project-id",
                project_id,
                "--name",
                "Smoke workspace",
                "--path",
                str(workspace),
                "--kind",
                "main",
            )
            created = checked_cli(
                executable,
                profile,
                "terminal",
                "--no-start",
                "create",
                "--workspace-id",
                workspace_id,
            )
            session_id = created["attachment"]["sessionId"]
            tab_id = created["tab"]["id"]
            checks.append("isolated Host PTY created")

            stream = StreamClient(executable, profile)
            lease_id = "native-" + run_marker
            observe = {
                "sessionId": session_id,
                "workspaceId": workspace_id,
                "tabId": tab_id,
                "viewLeaseId": lease_id,
            }
            stream.send(1, "observe", observe)
            pre_ack = []
            response = stream.until(lambda item: item.get("id") == 1, ignored=pre_ack)
            if not response.get("ok") or response.get("payload", {}).get("running") is not True:
                raise SmokeError("Isolated Host refused observation of existing PTY")
            if response["payload"].get("created") is not False:
                raise SmokeError("Observe unexpectedly reported a newly created process")
            checks.append("no-mint existing-session observation")

            # An old or copied viewLeaseId must never be allowed to type into
            # the running Terminal, even if the sessionId is correct.
            unsafe = {
                **observe,
                "viewLeaseId": "not-the-observed-lease",
                "dataBase64": base64.b64encode(b"echo should-not-run\r").decode(),
            }
            stream.send(2, "write", unsafe)
            denied = stream.until(lambda item: item.get("id") == 2)
            if denied.get("ok"):
                raise SmokeError("Host accepted a write using a different Native view lease")
            checks.append("per-key Host view lease enforced")

            marker = "SMOKE_OUTPUT_" + run_marker
            command = ("echo " + marker + "\r").encode("utf-8")
            stream.send(
                3,
                "write",
                {**observe, "dataBase64": base64.b64encode(command).decode()},
            )
            observed = list(pre_ack)
            output = bytearray()
            got_write_ack = False
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and (not got_write_ack or marker.encode() not in output):
                event = stream.until(lambda _: True, timeout=max(0.1, deadline - time.monotonic()))
                observed.append(event)
                if event.get("id") == 3:
                    if not event.get("ok"):
                        raise SmokeError("Scoped Host PTY write failed")
                    got_write_ack = True
                if event.get("event") == "output" and event.get("payload", {}).get("sessionId") == session_id:
                    output.extend(base64.b64decode(event["payload"]["dataBase64"]))
            if not got_write_ack or marker.encode() not in output:
                raise SmokeError("Did not receive PTY output on Native's persistent stream")
            checks.append("live PTY write ACK + pushed output roundtrip")

            stream.send(
                4,
                "observe",
                {
                    **observe,
                    "sessionId": "missing-" + run_marker,
                    "viewLeaseId": "native-missing-" + run_marker,
                },
            )
            missing = stream.until(lambda item: item.get("id") == 4)
            if missing.get("ok"):
                raise SmokeError("Missing Terminal was minted by observeExisting")
            checks.append("missing PTY cannot be created by observation")
            if b"should-not-run" in output:
                raise SmokeError("A denied lease unexpectedly wrote into the PTY")

            # CLI socket EOF is a view detach, not a Host-owned process kill.
            stream.close()
            stream = None
            read = checked_cli(
                executable, profile, "terminal", "--no-start", "read", "--handle", session_id
            )
            if read.get("running") is not True:
                raise SmokeError("Closing Native stream unexpectedly killed its Host PTY")
            checks.append("closing CLI stream preserves Host PTY")

            # Reattach A on a *new* Host TCP connection/CLI child, with a fresh
            # view lease. A stale view lease must never be reusable across
            # Native window closes, but the Host PTY must remain running.
            stream = StreamClient(executable, profile)
            fresh = {**observe, "viewLeaseId": "reconnected-" + run_marker}
            stream.send(11, "observe", fresh)
            fresh_ack = stream.until(lambda item: item.get("id") == 11)
            if not fresh_ack.get("ok") or fresh_ack.get("payload", {}).get("created") is not False:
                raise SmokeError("Existing Host PTY was not re-observable after stream close")
            write_and_expect_host_output(
                stream, 12, fresh,
                "echo REATTACHED_" + run_marker + "\r",
                "REATTACHED_" + run_marker,
            )
            checks.append("fresh Native view lease reconnects same Host PTY without remint")

            second = checked_cli(
                executable, profile, "terminal", "--no-start", "create",
                "--workspace-id", workspace_id,
            )
            second_session_id = second["attachment"]["sessionId"]
            second_identity = {
                "sessionId": second_session_id,
                "workspaceId": workspace_id,
                "tabId": second["tab"]["id"],
                "viewLeaseId": "second-pane-" + run_marker,
            }
            if second_identity["sessionId"] == session_id or second_identity["tabId"] == tab_id:
                raise SmokeError("Second PTY reused the first session or tab identity")
            stream.send(13, "observe", second_identity)
            second_ack = stream.until(lambda item: item.get("id") == 13)
            if not second_ack.get("ok") or second_ack.get("payload", {}).get("created") is not False:
                raise SmokeError("Second Native pane did not observe its own running PTY")
            write_and_expect_host_output(
                stream, 14, second_identity,
                "echo SECOND_PANE_" + run_marker + "\r",
                "SECOND_PANE_" + run_marker,
            )
            write_and_expect_host_output(
                stream, 15, fresh,
                "echo FIRST_PANE_" + run_marker + "\r",
                "FIRST_PANE_" + run_marker,
            )
            checks.append("two distinct PTYs share one CLI stream with scoped input/output")

            # More than a default 64KiB Host read window, from a real
            # interactive Windows cmd.exe. The terminal must deliver the
            # final line without a resync event or deadlocked private pipe.
            bulk_line = "BULK_" + run_marker + "_" + ("x" * 64)
            # The last line is not present in the *input* command text, so
            # detecting it proves the loop completed rather than matching
            # an echoed command prompt before its output arrives.
            bulk_marker = bulk_line + "_950"
            bulk = f"for /L %i in (1,1,950) do @echo {bulk_line}_%i\r"
            count = write_and_expect_host_output(
                stream, 16, second_identity, bulk, bulk_marker, timeout=40,
            )
            if count < 64 * 1024:
                raise SmokeError(
                    f"High-volume Host stream was shorter than 64KiB: {count} bytes"
                )
            checks.append("sustained >64KiB real PTY output arrives without resync")

            # Releasing one private Native connection must preserve both
            # Host-owned PTYs. The independent CLI read remains available.
            stream.close()
            stream = None
            for terminal_id in [session_id, second_identity["sessionId"]]:
                remaining = checked_cli(
                    executable, profile, "terminal", "--no-start", "read",
                    "--handle", terminal_id,
                )
                if remaining.get("running") is not True:
                    raise SmokeError("Closing multi-pane stream terminated a Host PTY")
            checks.append("closing dual-session Native stream preserves both Host PTYs")

            if native_app is not None:
                native_state = root / "gpui-state"
                prepare_native_preview_state(
                    native_state, workspace, session_id, workspace_id, tab_id,
                    second_session_id, second_identity["tabId"],
                )
                native_home = root / "native-home"
                native_home.mkdir()
                native_stderr_path = root / "isolated-native.stderr.log"
                with native_stderr_path.open("wb") as native_stderr:
                    native_env = {
                        **os.environ,
                        "ALERA_CLI_PATH": str(executable),
                        "ALERA_RUNTIME_DIR": str(profile),
                        "ALERA_GPUI_STATE_DIR": str(native_state),
                        "HOME": str(native_home),
                        "USERPROFILE": str(native_home),
                        "APPDATA": str(native_home / "AppData" / "Roaming"),
                        "LOCALAPPDATA": str(native_home / "AppData" / "Local"),
                    }
                    native_process = subprocess.Popen(
                        [str(native_app), str(workspace.resolve())],
                        cwd=workspace,
                        env=native_env,
                        stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL,
                        stderr=native_stderr,
                        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                    )
                    print(
                        f"ISOLATED_NATIVE_UI_READY pid={native_process.pid} "
                        f"workspace={workspace} seconds={hold_seconds}",
                        flush=True,
                    )
                    if native_reconnect:
                        old_pid, new_pid = interrupt_only_native_private_stream(
                            native_process, executable, profile
                        )
                        for terminal_id in (session_id, second_session_id):
                            state = checked_cli(
                                executable, profile, "terminal", "--no-start",
                                "read", "--handle", terminal_id,
                            )
                            if state.get("running") is not True:
                                raise SmokeError(
                                    "Native private relay restart terminated a Host PTY"
                                )
                        checks.append("Native private CLI stream automatically respawned; both Host PTYs survived")
                        print(
                            f"ISOLATED_NATIVE_STREAM_RECONNECTED old={old_pid} new={new_pid}",
                            flush=True,
                        )
                    until = time.monotonic() + hold_seconds
                    while time.monotonic() < until:
                        if native_process.poll() is not None:
                            raise SmokeError(
                                f"isolated GPUI window exited unexpectedly: "
                                f"code={native_process.returncode}"
                            )
                        time.sleep(0.2)
                checks.append("isolated Native GPUI window remained alive")
            return checks
        except Exception as error:
            diagnostics = "".join(host_diagnostics)[-900:].replace(token, "[redacted]").strip()
            raise SmokeError(
                f"{error}; isolated_host_exit={host.poll()}; "
                f"isolated_host_stderr={diagnostics or '(none)'}"
            ) from error
        finally:
            if native_process is not None and native_process.poll() is None:
                native_process.terminate()
                try:
                    native_process.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    native_process.kill()
                    native_process.wait(timeout=3)
            if stream is not None:
                stream.close()
            if control is not None:
                for terminal_id in (session_id, second_session_id):
                    if terminal_id is None:
                        continue
                    try:
                        host_rpc(control, "terminate", {"sessionId": terminal_id})
                    except (OSError, KeyError, SmokeError):
                        pass
                if os.name != "nt":
                    try:
                        host_rpc(control, "host.shutdown", {})
                    except (OSError, KeyError, SmokeError):
                        pass
            if os.name == "nt" and host.poll() is None:
                # PTY workers can deliberately outlive a graceful Host
                # shutdown for crash recovery. This Host is disposable:
                # stop ONLY the exact process tree we just created, while
                # its parent is still alive so taskkill /T can find the
                # worker and its ConPTY bootstrap grandchildren.
                try:
                    subprocess.run(
                        ["taskkill", "/T", "/F", "/PID", str(host.pid)],
                        stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        timeout=12,
                        check=False,
                    )
                except (OSError, subprocess.TimeoutExpired):
                    pass
            if host.poll() is None:
                try:
                    host.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    host.terminate()
                    try:
                        host.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        host.kill()
                        host.wait(timeout=3)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cli", type=Path, help="compiled Alera CLI (experimental)")
    parser.add_argument("--native-app", type=Path, help="optional GPUI exe to launch with an isolated preview tab")
    parser.add_argument("--native-reconnect", action="store_true",
                        help="kill only this Native GUI's scoped private relay and verify automatic respawn")
    parser.add_argument("--hold-seconds", type=int, default=25)
    args = parser.parse_args()
    if not args.cli.is_file() or (args.native_app is not None and not args.native_app.is_file()):
        parser.error("CLI and optional Native app paths must be existing executable files")
    if not 1 <= args.hold_seconds <= 180:
        parser.error("--hold-seconds must be between 1 and 180")
    if args.native_reconnect and args.native_app is None:
        parser.error("--native-reconnect requires --native-app")
    try:
        results = run_smoke(
            args.cli.resolve(), native_app=args.native_app.resolve()
            if args.native_app is not None else None,
            hold_seconds=args.hold_seconds,
            native_reconnect=args.native_reconnect,
        )
    except (SmokeError, OSError, KeyError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"FAIL: isolated Terminal stream smoke: {error}", file=sys.stderr)
        sys.exit(1)
    print(f"PASS: {len(results)} isolated Host/CLI stream contracts")
    for item in results:
        print(" - " + item)
