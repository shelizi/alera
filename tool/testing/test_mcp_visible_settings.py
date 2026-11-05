"""Contract tests; these do not constitute native GUI acceptance evidence."""
import unittest

from mcp_visible_settings import observe_change, visible_editor


def snapshot(value="4", *, foreground=True, visible=True, offscreen=False):
    return {"windows": [
        {"pid": 11, "hwnd": 101, "visible": True, "minimized": False,
         "foreground": False, "fields": []},
        {"pid": 22, "hwnd": 202, "visible": visible, "minimized": False,
         "foreground": foreground, "fields": [
             {"value": value, "offscreen": offscreen, "contained": True}]},
    ]}


class VisibleSettingsTests(unittest.TestCase):
    def test_accepts_only_visible_foreground_native_field(self):
        self.assertEqual(visible_editor(snapshot(), "4"), (22, 202))
        for options in ({"foreground": False}, {"visible": False},
                        {"offscreen": True}, {"value": "5"}):
            with self.subTest(options=options), self.assertRaises(AssertionError):
                visible_editor(snapshot(**options), "4")
        for section, key in (("windows", "minimized"), ("fields", "contained")):
            data = snapshot()
            if section == "windows":
                data["windows"][1][key] = True
            else:
                data["windows"][1]["fields"][0][key] = False
            with self.assertRaises(AssertionError):
                visible_editor(data, "4")

    def test_requires_two_distinct_processes_and_unique_field(self):
        for key in ("pid", "hwnd"):
            data = snapshot()
            data["windows"][0][key] = data["windows"][1][key]
            with self.assertRaises(AssertionError):
                visible_editor(data, "4")
        data = snapshot()
        data["windows"][0]["fields"] = data["windows"][1]["fields"]
        with self.assertRaises(AssertionError):
            visible_editor(data, "4")

    def test_cas_observation_and_rollback_never_read_window_b(self):
        calls = []
        values = iter([snapshot("5"), snapshot("4")])
        def mcp(tool, args, ok):
            calls.append((tool, args))
            return {"value": args.get("value", 5)}
        observe_change(mcp, lambda: next(values), ["a", "b"], 4, (22, 202))
        self.assertTrue(all(args["windowId"] == "a" for _, args in calls))
        updates = [args for tool, args in calls if tool == "alera.update_setting"]
        self.assertEqual([(x["expectedValue"], x["value"]) for x in updates],
                         [(4, 5), (5, 4)])

    def test_missing_evidence_still_rolls_back_and_never_succeeds(self):
        calls = []
        evidence = {}
        def mcp(tool, args, ok):
            calls.append((tool, args))
            return {"value": args.get("value", 5)}
        with self.assertRaises(AssertionError):
            observe_change(mcp, lambda: snapshot("4"), ["a", "b"],
                           4, (22, 202), timeout=0, evidence=evidence)
        self.assertEqual(calls[-1][1]["value"], 4)
        self.assertFalse(evidence["targetVisible"])
        self.assertTrue(evidence["rollbackCommitted"])
        self.assertTrue(evidence["rollbackVisible"])

    def test_changed_native_identity_is_rejected(self):
        with self.assertRaises(AssertionError):
            visible_editor(snapshot("5"), "5", identity=(33, 303))

    def test_lost_mutation_ack_still_attempts_cas_rollback(self):
        calls = []
        def mcp(tool, args, ok):
            calls.append(args)
            if len(calls) == 1:
                raise AssertionError("lost ACK after persistence")
            return {"value": args.get("value", 5)}
        with self.assertRaisesRegex(AssertionError, "lost ACK"):
            observe_change(mcp, lambda: snapshot("4"), ["a", "b"],
                           4, (22, 202), timeout=0)
        self.assertEqual(calls[-1]["expectedValue"], 5)
        self.assertEqual(calls[-1]["value"], 4)

    def test_rollback_does_not_overwrite_unrelated_concurrent_edit(self):
        calls = []
        def mcp(tool, args, ok):
            calls.append(tool)
            return {"value": args.get("value", 6)}
        with self.assertRaisesRegex(AssertionError, "unexpected concurrent"):
            observe_change(mcp, lambda: snapshot("5"), ["a", "b"],
                           4, (22, 202), timeout=0)
        self.assertEqual(calls.count("alera.update_setting"), 1)

    def test_maximum_tab_size_uses_in_range_target_and_restores(self):
        updates = []
        frames = iter([snapshot("7"), snapshot("8")])
        def mcp(tool, args, ok):
            if tool == "alera.update_setting":
                updates.append(args["value"])
            return {"value": args.get("value", 7)}
        observe_change(mcp, lambda: next(frames), ["a", "b"], 8, (22, 202))
        self.assertEqual(updates, [7, 8])


if __name__ == "__main__":
    unittest.main()
