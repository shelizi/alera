"""Pure, non-Host regression tests for isolated Native Terminal load probes."""

import unittest
from pathlib import Path

from native_terminal_visible_activity_probe import changed_pixels, pane_content_crops
from native_terminal_visual_latency_probe import percentile
from terminal_stream_isolated_smoke import (
    SmokeError,
    native_visible_cursors_from_diagnostic,
    native_primary_cursor_from_diagnostic,
    paced_load_command,
    real_tui_less_command,
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

    def test_real_tui_accepts_one_visible_pane_and_background_tab(self):
        line = (
            "GPUI_STREAM_DIAG clock=99 stream=true updates=0 bytes=0 q=0 "
            "active=1 pending=0 tabs=[(7630, true), (0, true)]"
        )
        self.assertEqual(native_primary_cursor_from_diagnostic(line), 7630)
        for broken in (
            line.replace("active=1", "active=0"),
            line.replace("(7630, true)", "(7630, false)"),
            line.replace("tabs=[(7630, true), (0, true)]", "tabs=[]"),
        ):
            with self.subTest(broken=broken), self.assertRaises(SmokeError):
                native_primary_cursor_from_diagnostic(broken)

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

    def test_real_tui_uses_read_only_pager_with_quoted_isolated_file(self):
        pager = Path(r"C:\Program Files\Git\usr\bin\less.exe")
        document = Path(r"C:\temp\isolated gui\input.txt")
        command = real_tui_less_command(pager, document)
        self.assertEqual(
            command,
            r'"C:\Program Files\Git\usr\bin\less.exe" -R -- "C:\temp\isolated gui\input.txt"',
        )
        self.assertNotIn("REAL_TUI_ROW_240", command)

    def test_real_tui_rejects_other_programs_and_shell_metacharacters(self):
        document = Path(r"C:\temp\isolated\input.txt")
        for pager in (
            Path(r"C:\Windows\System32\cmd.exe"),
            Path(r"C:\Program Files\Git\usr\bin\less.exe & echo unsafe"),
            Path("less.exe"),
        ):
            with self.subTest(pager=pager), self.assertRaises(ValueError):
                real_tui_less_command(pager, document)


if __name__ == "__main__":
    unittest.main()
