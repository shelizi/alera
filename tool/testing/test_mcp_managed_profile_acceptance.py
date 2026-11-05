"""Offline regressions. No Host, provider, account or network is used."""
import copy
import hashlib
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import mcp_managed_profile_acceptance as acceptance


def evidence(provider="claude"):
    return {
        "provider": provider, "launchMethod": "agentProfile.launch",
        "launchMode": "managed", "profileId": "private-profile",
        "handle": "private-tab", "runId": "private-run",
        "mcpOptIn": "1", "providerPid": 100, "ptyShellPid": 90,
        "providerAncestors": [90, 80], "mcpPid": 110,
        "mcpAncestors": [100, 90, 80], "transientMcpConfig": True,
        "nativeHookRelay": True, "promptConsumer": "provider",
        "syntheticStatus": False, "recipientWritesAfterLaunch": 0,
        "trustBypass": False, "accountModified": False,
        "globalConfigModified": False, "privateHostStopped": True,
        "privateFilesRemoved": True, "challenge": "9a1e72c4d06f",
        "replySha256": hashlib.sha256(b"F60D4C27E1A9").hexdigest(),
        "events": [
            {"kind": kind, "runId": "private-run", "handle": "private-tab",
             "providerPid": 100, "sequence": i}
            for i, kind in enumerate((
                "profile_launch", "native_prompt", "model_mcp_read_result",
                "message_queued", "native_stop", "host_pty_delivery",
                "native_prompt", "model_reply", "native_stop",
            ))
        ],
        "deliveryCompletionConfirmed": False,
    }


class AcceptanceTests(unittest.TestCase):
    def test_both_provider_contracts_are_assessed_separately(self):
        for provider in ("claude", "codex"):
            with self.subTest(provider=provider):
                report = acceptance.assess(evidence(provider))
                self.assertTrue(report["evidenceContractSatisfied"])
                self.assertFalse(report["liveRunExecutedByThisCommand"])
                self.assertEqual(report["provider"], provider)

    def test_direct_launch_synthetic_hooks_and_adapter_second_turn_fail(self):
        for key, value in (
            ("launchMethod", "direct_claude"), ("launchMode", "command"),
            ("mcpOptIn", "true"), ("syntheticStatus", True),
            ("promptConsumer", "python"), ("recipientWritesAfterLaunch", 1),
            ("nativeHookRelay", False), ("transientMcpConfig", False),
            ("trustBypass", True), ("accountModified", True),
            ("globalConfigModified", True), ("privateHostStopped", False),
            ("privateFilesRemoved", False), ("providerAncestors", [80]),
            ("mcpAncestors", [90, 80]), ("replySha256", "wrong"),
            ("deliveryCompletionConfirmed", True), ("provider", "other"),
            ("providerPid", True), ("recipientWritesAfterLaunch", False),
        ):
            with self.subTest(key=key):
                data = evidence()
                data[key] = value
                self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"])

    def test_missing_reordered_stale_or_cross_pty_events_fail(self):
        for index in range(len(evidence()["events"])):
            data = evidence()
            del data["events"][index]
            self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"])
        for key, value in (("runId", "old"), ("handle", "other"),
                           ("providerPid", 999), ("sequence", -1)):
            data = evidence()
            data["events"][6][key] = value
            self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"])
        data = evidence()
        data["events"][3], data["events"][4] = data["events"][4], data["events"][3]
        self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"])

    def test_missing_and_malformed_evidence_is_fail_closed(self):
        for data in (None, [], {}, {"events": None}, {"events": [None]}):
            self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"])
        for key in evidence():
            data = evidence()
            del data[key]
            self.assertFalse(acceptance.assess(data)["evidenceContractSatisfied"], key)

    def test_report_does_not_echo_input_or_private_evidence(self):
        data = evidence()
        data["secret"] = "never-return-this"
        data["events"][0]["prompt"] = "never-return-this"
        before = copy.deepcopy(data)
        self.assertNotIn("never-return-this", str(acceptance.assess(data)))
        self.assertEqual(data, before)

    def test_default_command_never_launches_or_reads_a_fixture(self):
        with patch("builtins.print") as output, patch.object(Path, "read_text") as read:
            self.assertEqual(acceptance.main([]), 0)
            read.assert_not_called()
            self.assertIn('"status": "not_requested"', output.call_args.args[0])

    def test_managed_tab_requires_production_snapshot_and_profile_identity(self):
        tab = {"id": "tab", "workspaceId": "workspace", "kind": "terminal",
               "payload": {"terminalSessionId": "tab", "agentProfileLaunchV1": {
                   "version": 1, "agentType": "claude", "launchMode": "managed",
                   "profile": {"id": "profile", "revision": 0},
                   "launch": {"kind": "managed", "executable": "claude", "argv": []},
               }}}
        self.assertTrue(acceptance.managed_tab(tab, "tab", "workspace", "claude"))
        for key, value in (("version", True), ("version", 2), ("launchMode", "command"),
                           ("agentType", "codex"), ("profile", {}),
                           ("launch", {"kind": "command", "command": "claude"})):
            invalid = copy.deepcopy(tab)
            invalid["payload"]["agentProfileLaunchV1"][key] = value
            self.assertFalse(acceptance.managed_tab(invalid, "tab", "workspace", "claude"))
        self.assertFalse(acceptance.managed_tab(tab, "other", "workspace", "claude"))
        self.assertFalse(acceptance.managed_tab(tab, "tab", "other", "claude"))
        self.assertFalse(acceptance.managed_tab({}, "tab", "workspace", "claude"))

    def test_direct_harness_default_is_preflight_only(self):
        import mcp_managed_claude_interactive_autonomy_e2e as direct
        with patch.object(direct, "driver") as driver, patch("builtins.print"), \
                patch("sys.argv", ["direct-harness"]):
            direct.main()
            driver.assert_not_called()

    def test_direct_driver_refuses_before_any_host_or_report_mutation(self):
        import mcp_managed_claude_interactive_autonomy_e2e as direct
        with patch.object(direct, "private_host_start") as start, \
                patch.object(Path, "unlink") as unlink:
            with self.assertRaises(RuntimeError):
                direct.driver(Path("missing"))
            start.assert_not_called()
            unlink.assert_not_called()

    def test_internal_recipient_mode_cannot_silently_launch_a_model(self):
        import mcp_managed_claude_interactive_autonomy_e2e as direct
        with patch.object(direct, "recipient") as recipient, \
                patch("sys.argv", ["direct-harness", "--mode", "recipient"]):
            with self.assertRaises(RuntimeError):
                direct.main()
            recipient.assert_not_called()

    def test_direct_model_requires_preprovisioned_isolated_account(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            self.assertFalse(acceptance.isolated_provider_home(home))
            (home / ".alera-isolated-provider-fixture").write_text(
                "alera-mcp-isolated-provider-fixture-v1", encoding="utf-8")
            # Windows may place tempfile directories beneath the real user
            # home, which production must reject despite the fixture marker.
            if home.resolve().is_relative_to(Path.home().resolve()):
                self.assertFalse(acceptance.isolated_provider_home(home))
            with patch.object(Path, "home", return_value=home.parent / "unrelated-user-home"):
                self.assertTrue(acceptance.isolated_provider_home(home))
            self.assertFalse(acceptance.isolated_provider_home(Path.home()))
            self.assertFalse(acceptance.isolated_provider_home(Path(__file__).resolve().parents[2]))

    def test_preflight_is_read_only_and_requires_isolated_existing_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            for name in ("runtime", "workspace", "account"):
                (base / name).mkdir()
            fixture = {
                "root": str(base), "runtime": str(base / "runtime"),
                "workspace": str(base / "workspace"), "account": str(base / "account"),
                "provider": "claude", "profileId": "profile",
                "launchMode": "managed", "mcpOptIn": "1",
            }
            before = sorted(str(p) for p in base.rglob("*"))
            report = acceptance.preflight(fixture)
            self.assertTrue(report["readyForExplicitRun"])
            self.assertFalse(report["liveRunExecutedByThisCommand"])
            self.assertEqual(before, sorted(str(p) for p in base.rglob("*")))
            for key, value in (("account", str(base.parent)),
                               ("runtime", str(base)),
                               ("workspace", str(base / "runtime")),
                               ("mcpOptIn", "true"), ("launchMode", "command")):
                invalid = {**fixture, key: value}
                self.assertFalse(acceptance.preflight(invalid)["readyForExplicitRun"])


if __name__ == "__main__":
    unittest.main()
