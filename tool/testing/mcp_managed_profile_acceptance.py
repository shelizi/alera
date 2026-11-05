"""Read-only preflight and evidence review for explicit MANAGED acceptance.

This command never launches a Host/model, approves trust, installs hooks, or
reads credentials. Supplied evidence is an assertion contract, not authenticated
telemetry. Passing it cannot establish that a real production run took place.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re


LIFECYCLE = (
    "profile_launch", "native_prompt", "model_mcp_read_result",
    "message_queued", "native_stop", "host_pty_delivery",
    "native_prompt", "model_reply", "native_stop",
)


def _positive_pid(value: object) -> bool:
    return type(value) is int and value > 0


def _ancestors(value: object, pid: object) -> bool:
    return (isinstance(value, list) and bool(value)
            and all(_positive_pid(item) for item in value) and pid in value)


def managed_tab(value: object, handle: str, workspace: str, provider: str) -> bool:
    """Require the immutable production Managed Profile snapshot, not an agent label."""
    if not isinstance(value, dict) or provider not in ("claude", "codex"):
        return False
    payload = value.get("payload")
    if not isinstance(payload, dict):
        return False
    snapshot = payload.get("agentProfileLaunchV1")
    if not isinstance(snapshot, dict):
        return False
    profile, launch = snapshot.get("profile"), snapshot.get("launch")
    return (value.get("id") == handle and value.get("workspaceId") == workspace
            and value.get("kind") == "terminal" and payload.get("terminalSessionId") == handle
            and type(snapshot.get("version")) is int and snapshot["version"] == 1
            and snapshot.get("launchMode") == "managed" and snapshot.get("agentType") == provider
            and isinstance(profile, dict) and isinstance(profile.get("id"), str)
            and bool(profile["id"].strip()) and type(profile.get("revision")) is int
            and profile["revision"] >= 0 and isinstance(launch, dict)
            and launch.get("kind") == "managed" and launch.get("executable") == provider
            and isinstance(launch.get("argv"), list)
            and all(isinstance(arg, str) for arg in launch["argv"]))


def isolated_provider_home(value: object) -> bool:
    """Require an explicitly preprovisioned disposable account; never create/copy one."""
    try:
        if not isinstance(value, (str, Path)) or not Path(value).is_absolute():
            return False
        home = Path(value).resolve(strict=True)
        default = Path.home().resolve()
        repo = Path(__file__).resolve().parents[2]
        if (not home.is_dir() or home.parent == home
                or home.is_relative_to(default) or default.is_relative_to(home)
                or home.is_relative_to(repo) or repo.is_relative_to(home)
                or "dropbox" in [part.lower() for part in home.parts]):
            return False
        return (home / ".alera-isolated-provider-fixture").read_text(encoding="utf-8").strip() == (
            "alera-mcp-isolated-provider-fixture-v1")
    except (OSError, ValueError, RuntimeError):
        return False


def assess(value: object) -> dict:
    """Reject weaker test scopes and incomplete or mixed-run lifecycle evidence."""
    data = value if isinstance(value, dict) else {}
    provider = data.get("provider")
    checks = {
        "supportedProvider": provider in ("claude", "codex"),
        "productionManagedLaunch": (
            data.get("launchMethod") in ("agentProfile.launch", "agentProfile.launchIdempotent")
            and data.get("launchMode") == "managed"
            and isinstance(data.get("profileId"), str) and bool(data["profileId"].strip())
        ),
        "explicitInjection": data.get("mcpOptIn") == "1"
            and data.get("transientMcpConfig") is True,
        "providerBoundToPty": _positive_pid(data.get("providerPid"))
            and _positive_pid(data.get("ptyShellPid"))
            and data.get("providerPid") != data.get("ptyShellPid")
            and _ancestors(data.get("providerAncestors"), data.get("ptyShellPid")),
        "mcpChildBoundToProvider": _positive_pid(data.get("mcpPid"))
            and data.get("mcpPid") != data.get("providerPid")
            and _ancestors(data.get("mcpAncestors"), data.get("providerPid")),
        "nativeProviderOwnsSecondTurn": data.get("nativeHookRelay") is True
            and data.get("syntheticStatus") is False
            and data.get("promptConsumer") == "provider"
            and type(data.get("recipientWritesAfterLaunch")) is int
            and data["recipientWritesAfterLaunch"] == 0,
        "deliveryIsNotCompletion": data.get("deliveryCompletionConfirmed") is False,
        "noTrustOrAccountMutation": all(data.get(key) is False for key in (
            "trustBypass", "accountModified", "globalConfigModified")),
        "cleanupConfirmed": all(data.get(key) is True for key in (
            "privateHostStopped", "privateFilesRemoved")),
    }
    events = data.get("events")
    identity = all(isinstance(data.get(key), str) and bool(data[key].strip())
                   for key in ("runId", "handle"))
    checks["orderedSameRunLifecycle"] = (
        identity and isinstance(events, list) and len(events) == len(LIFECYCLE)
        and all(isinstance(event, dict)
                and event.get("kind") == kind
                and type(event.get("sequence")) is int and event["sequence"] == index
                and event.get("runId") == data["runId"]
                and event.get("handle") == data["handle"]
                and type(event.get("providerPid")) is int
                and event["providerPid"] == data.get("providerPid")
                for index, (event, kind) in enumerate(zip(events, LIFECYCLE)))
    )
    challenge = data.get("challenge")
    checks["nonceDerivedSecondReply"] = (
        isinstance(challenge, str) and re.fullmatch(r"[a-f0-9]{12,64}", challenge) is not None
        and data.get("replySha256") == hashlib.sha256(
            challenge[::-1].upper().encode("ascii")).hexdigest()
    )
    return {
        "provider": provider if provider in ("claude", "codex") else "unsupported",
        "checks": checks, "evidenceContractSatisfied": all(checks.values()),
        "liveRunExecutedByThisCommand": False,
        "scope": "offline supplied-evidence review; not a real-model acceptance result",
    }


def preflight(value: object) -> dict:
    """Check declared fixture isolation without contacting or changing the Host."""
    data = value if isinstance(value, dict) else {}
    checks = {
        "supportedProvider": data.get("provider") in ("claude", "codex"),
        "managedProfile": data.get("launchMode") == "managed"
            and isinstance(data.get("profileId"), str) and bool(data["profileId"].strip()),
        "explicitInjection": data.get("mcpOptIn") == "1",
        "isolatedPaths": False,
    }
    try:
        raw_paths = [data.get(key) for key in ("root", "runtime", "workspace", "account")]
        if not all(isinstance(path, str) and Path(path).is_absolute() for path in raw_paths):
            raise ValueError("absolute existing fixture directories required")
        root, *children = [Path(path).resolve(strict=True) for path in raw_paths]
        repo = Path(__file__).resolve().parents[2]
        home = Path.home().resolve()
        checks["isolatedPaths"] = (
            root.is_dir() and root.parent != root and root != home
            and not root.is_relative_to(repo) and not repo.is_relative_to(root)
            and "dropbox" not in [part.lower() for part in root.parts]
            and all(child.is_dir() and child != root and child.is_relative_to(root)
                    and child != home for child in children)
            and all(not a.is_relative_to(b) and not b.is_relative_to(a)
                    for i, a in enumerate(children) for b in children[i + 1:])
        )
    except (OSError, ValueError, TypeError, RuntimeError):
        pass
    return {
        "checks": checks, "readyForExplicitRun": all(checks.values()),
        "liveRunExecutedByThisCommand": False,
        "scope": "declared fixture preflight only; Host environment, auth and trust unverified",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--preflight", type=Path, help="Review an isolated fixture manifest")
    group.add_argument("--evidence", type=Path, help="Review sanitized evidence from one provider/run")
    args = parser.parse_args(argv)
    if not args.preflight and not args.evidence:
        print(json.dumps({"liveRunExecutedByThisCommand": False,
                          "status": "not_requested", "mcpDefault": "off"}))
        return 0
    try:
        data = json.loads((args.preflight or args.evidence).read_text(encoding="utf-8"))
        report = preflight(data) if args.preflight else assess(data)
    except (OSError, ValueError):
        print(json.dumps({"status": "invalid_input", "liveRunExecutedByThisCommand": False}))
        return 1
    print(json.dumps(report, indent=2))
    return 0 if report.get("readyForExplicitRun", report.get("evidenceContractSatisfied")) else 1


if __name__ == "__main__":
    raise SystemExit(main())
