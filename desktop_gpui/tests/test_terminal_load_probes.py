"""Pure, non-Host regression tests for isolated Native Terminal load probes."""

import unittest

from native_terminal_visible_activity_probe import changed_pixels, pane_content_crops
from native_terminal_visual_latency_probe import percentile
from terminal_stream_isolated_smoke import (
    SmokeError,
    native_visible_cursors_from_diagnostic,
    paced_load_command,
)


class IsolatedTerminalLoadProbeTests(unittest.TestCase):
    def test_echoed_command_cannot_contain_completion_marker_in_either_style(self):
        for style in ("plain", "ansi"):
            for pane in (0, 1):
                with self.subTest(style=style, pane=pane):
                    marker, command = paced_load_command("test1abc", pane, 30, style)
                    self.assertEqual(marker, f"LOAD_DONE_test1abc_{pane}")
                    self.assertNotIn(marker, command)
                    self.assertIn("-lt 30", command)
                    self.assertIn("'LOAD_'+'DONE_test1abc_", command)

    def test_ansi_tui_uses_alternate_screen_cursor_color_and_restore(self):
        _, command = paced_load_command("test1abc", 1, 30, "ansi")
        for token in ("[?1049h", "[?1049l", "[?25l", "[?25h", "[3", "[K"):
            with self.subTest(token=token):
                self.assertIn(token, command)

    def test_unknown_workload_style_is_refused(self):
        with self.assertRaises(ValueError):
            paced_load_command("test1abc", 0, 30, "unexpected")

    def test_two_visible_stream_cursors_require_two_live_subscriptions(self):
        good = (
            "GPUI_STREAM_DIAG clock=99 stream=true updates=0 bytes=0 q=0 "
            "active=2 pending=0 tabs=[(111116, true), (323849, true)]"
        )
        self.assertEqual(native_visible_cursors_from_diagnostic(good, 2), [111116, 323849])
        with self.assertRaises(SmokeError):
            native_visible_cursors_from_diagnostic(good.replace("active=2", "active=1"), 2)
        with self.assertRaises(SmokeError):
            native_visible_cursors_from_diagnostic(good.replace("323849, true", "323849, false"), 2)

    def test_two_pane_pixel_crops_do_not_overlap(self):
        primary, secondary = pane_content_crops((700, 200, 2400, 1600))
        self.assertLessEqual(primary["left"] + primary["width"], secondary["left"])
        self.assertGreater(primary["width"], 0)
        self.assertGreater(secondary["height"], 0)

    def test_visible_change_requires_real_pixel_difference(self):
        import numpy as np

        before = np.zeros((30, 30, 3), dtype=np.uint8)
        self.assertEqual(changed_pixels(before, before), 0)
        after = np.full((30, 30, 3), 255, dtype=np.uint8)
        self.assertEqual(changed_pixels(before, after), 900)

    def test_latency_percentiles_interpolate_without_out_of_range(self):
        self.assertEqual(percentile([4, 1, 3, 2], 0.5), 2.5)
        self.assertAlmostEqual(percentile([4, 1, 3, 2], 0.95), 3.85)


if __name__ == "__main__":
    unittest.main()
