#!/usr/bin/env python3
"""Opt-in, pixel-only foreground activity check for *both* disposable GPUI panes.

Run while `terminal_stream_isolated_smoke.py --native-split-gate
--native-load-style ansi` is actively emitting concurrent ANSI/TUI updates.
This is not a frame-rate or end-to-end key latency benchmark. It checks that
both visible pane regions really change while output arrives, rather than
relying on terminal cursor offsets alone. Captures stay in memory; no pixels,
PTY text, Host token, or screenshots are persisted.
"""

import argparse
import json
import os
import statistics
import time

from native_terminal_visual_latency_probe import isolated_window, physical_rect


def pane_content_crops(rect):
    left, top, width, height = rect
    # The ANSI/TUI test addresses rows at the TOP of each terminal screen;
    # avoid Windows title bar, tabs, left/right sidebar, bottom status bar.
    return [
        {
            "left": left + int(width * pane_x),
            "top": top + int(height * 0.19),
            "width": int(width * pane_width),
            "height": int(height * 0.45),
        }
        for pane_x, pane_width in ((0.267, 0.226), (0.502, 0.213))
    ]


def changed_pixels(previous, current, threshold=28):
    import numpy as np

    return int((np.max(np.abs(current.astype(np.int16) - previous.astype(np.int16)), axis=2)
                > threshold).sum())


def sample(pid, count, interval_seconds, min_changed):
    import mss
    import numpy as np

    with mss.MSS() as capture:
        bounds = physical_rect(isolated_window(pid))
        crops = pane_content_crops(bounds)

        def screenshot(region):
            return np.asarray(capture.grab(region))[:, :, :3].copy()

        previous = [screenshot(region) for region in crops]
        progress = [0, 0]
        changed = [[], []]
        first_sample = time.monotonic()
        for _ in range(count):
            time.sleep(interval_seconds)
            isolated_window(pid)  # fails closed if focus/process changed
            if physical_rect(isolated_window(pid)) != bounds:
                raise RuntimeError("Disposable GPUI window geometry changed during probe")
            for index, region in enumerate(crops):
                now = screenshot(region)
                delta = changed_pixels(previous[index], now)
                # Cursor blinking or Windows caret animation is much smaller
                # than hundreds of colored TUI cells being updated.
                threshold = max(650, int(region["width"] * region["height"] * 0.002))
                if delta >= threshold:
                    progress[index] += 1
                changed[index].append(delta)
                previous[index] = now

        result = {
            "metric": "disposable-GPUI-concurrent-visible-pane-changes",
            "elapsed_seconds": round(time.monotonic() - first_sample, 2),
            "samples": count,
            "sample_interval_ms": round(interval_seconds * 1000),
            "changed_samples_primary": progress[0],
            "changed_samples_secondary": progress[1],
            "median_changed_pixels_primary": statistics.median(changed[0]),
            "median_changed_pixels_secondary": statistics.median(changed[1]),
            "capture_quantized": True,
        }
        print("ISOLATED_GPUI_VISIBLE_ACTIVITY", json.dumps(result), flush=True)
        if any(value < min_changed for value in progress):
            raise RuntimeError(
                f"A Native Terminal viewport was not visibly refreshed under ANSI load: "
                f"changes={progress}; minimum={min_changed}. Ensure the probe ran while "
                "both PTYs were actually producing output."
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--samples", type=int, default=14)
    parser.add_argument("--interval-ms", type=int, default=500)
    parser.add_argument("--min-changed-samples", type=int, default=3)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows GUI-only test")
    if not 4 <= args.samples <= 100 or not 100 <= args.interval_ms <= 2000:
        parser.error("samples must be 4..100 and interval-ms 100..2000")
    if not 1 <= args.min_changed_samples <= args.samples:
        parser.error("min-changed-samples must be 1..samples")
    sample(args.pid, args.samples, args.interval_ms / 1000, args.min_changed_samples)


if __name__ == "__main__":
    main()
