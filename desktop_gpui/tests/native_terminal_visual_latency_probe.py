#!/usr/bin/env python3
"""Measure real visible GPUI Terminal response to scoped ASCII key presses.

This opt-in Windows probe targets ONLY a currently foreground, disposable
alera-gpui-desktop.exe process launched by terminal_stream_isolated_smoke.py.
It does not use OCR, patch the Host, inject into other apps, or open any Shell.

Measurement: Windows key-down -> first changed pixels in the selected
Terminal pane, sampled using MSS (not a real present-hook/photodiode). Includes
Native input dispatch, CLI/Host, VT repaint and desktop capture quantization.
Reported p50/p95 therefore have capture-rate uncertainty, not guaranteed
frame-present timestamps. Leave the isolated cmd.exe prompt empty beforehand.
"""

import argparse
import ctypes
import json
import math
import os
from pathlib import Path
import statistics
import time


def percentile(values, fraction):
    if not values:
        raise ValueError("no samples")
    data = sorted(values)
    rank = (len(data) - 1) * fraction
    lower = math.floor(rank)
    higher = math.ceil(rank)
    return data[lower] + (data[higher] - data[lower]) * (rank - lower)


def isolated_window(pid):
    import psutil
    import win32gui
    import win32process

    process = psutil.Process(pid)
    if Path(process.exe()).name.lower() != "alera-gpui-desktop.exe":
        raise RuntimeError("Refusing to send input: PID is not Native GPUI")
    if not any("alera_gpui_stream_isolated_" in arg.lower()
               for arg in process.cmdline()):
        raise RuntimeError("Refusing to send input: Native is not from isolated smoke")
    matches = []

    def visit(hwnd, _):
        if (win32gui.IsWindowVisible(hwnd)
                and win32process.GetWindowThreadProcessId(hwnd)[1] == pid):
            matches.append(hwnd)

    win32gui.EnumWindows(visit, None)
    # Windows IME / GPUI popup may temporarily own a second tiny top-level
    # surface in the *same* process. Do not confuse it with the workbench.
    if not matches:
        raise RuntimeError("No visible isolated Native GPUI workbench")
    candidates = sorted(matches, key=lambda item: (
        (win32gui.GetWindowRect(item)[2] - win32gui.GetWindowRect(item)[0])
        * (win32gui.GetWindowRect(item)[3] - win32gui.GetWindowRect(item)[1])
    ), reverse=True)
    hwnd = candidates[0]
    bounds = win32gui.GetWindowRect(hwnd)
    if (bounds[2] - bounds[0] < 600 or bounds[3] - bounds[1] < 350):
        raise RuntimeError("Largest Native GUI window was not the workbench")
    if win32process.GetWindowThreadProcessId(win32gui.GetForegroundWindow())[1] != pid:
        raise RuntimeError("Refusing to send keys: another app has foreground focus")
    return hwnd


class Rect(ctypes.Structure):
    _fields_ = [(field, ctypes.c_long) for field in ("left", "top", "right", "bottom")]


def physical_rect(hwnd):
    # DWMWA_EXTENDED_FRAME_BOUNDS returns physical coordinates regardless of
    # Windows DPI-virtualized GetWindowRect, aligning with MSS's pixels.
    bounds = Rect()
    result = ctypes.windll.dwmapi.DwmGetWindowAttribute(
        ctypes.c_void_p(hwnd), 9, ctypes.byref(bounds), ctypes.sizeof(bounds)
    )
    if result != 0:
        raise RuntimeError("Cannot establish physical Native window bounds")
    width, height = bounds.right - bounds.left, bounds.bottom - bounds.top
    if width < 600 or height < 350:
        raise RuntimeError("Native window too small for dual-pane measurement")
    return bounds.left, bounds.top, width, height


def pane_crop(rect, pane):
    left, top, width, height = rect
    # Exclude the active tab strip, Explorer, second-pane toolbar and OS taskbar.
    # A cropped *pane* measurement won't confuse another pane's rendering.
    x, pane_width = (0.267, 0.226) if pane == "primary" else (0.502, 0.213)
    return {
        "left": left + int(width * x),
        "top": top + int(height * 0.44),
        "width": int(width * pane_width),
        # Include the final cmd prompt at ~96% of the GPUI window height.
        # Earlier 57..93% cropping missed new lines as the pane scrolled.
        "height": int(height * 0.53),
    }


def measure(pid, pane, samples, dry_run):
    import mss
    import numpy as np
    import win32api
    import win32con

    # MSS makes this process DPI-aware; the DWM bounds below are physical.
    with mss.MSS() as camera:
        hwnd = isolated_window(pid)
        rect = physical_rect(hwnd)
        crop = pane_crop(rect, pane)
        print("NATIVE_PANE_PROBE", json.dumps({"pid": pid, "pane": pane,
                                              "window": rect, "crop": crop}), flush=True)
        if dry_run:
            return

        def frame():
            return np.asarray(camera.grab(crop))[:, :, :3].copy()

        def send_key(vk):
            # Every SendInput is guarded against accidental focus transfer.
            isolated_window(pid)
            win32api.keybd_event(vk, 0, 0, 0)
            win32api.keybd_event(vk, 0, win32con.KEYEVENTF_KEYUP, 0)

        # Escape unfinished IME candidate composition in the disposable pane.
        # The test operator must switch Windows IME to English first: phonetic
        # compositions do not immediately echo into an interactive shell.
        send_key(win32con.VK_ESCAPE)
        time.sleep(0.25)
        baseline_capture_ms = []
        for _ in range(5):
            started = time.perf_counter()
            frame()
            baseline_capture_ms.append((time.perf_counter() - started) * 1000)

        observations = []
        # Enter on an already-full Windows cmd viewport can be pixel-identical
        # (the same prompt scrolls into exactly the previous prompt's place).
        # Use DIFFERENT ASCII characters without Enter so each sample has a
        # new glyph, then clear the unfinished test command with Escape.
        alphabet = "QWERTYUIOPASDFGHJKLZXCVBNM"
        difference_threshold = max(160, int(crop["width"] * crop["height"] * 0.0003))
        for index in range(samples):
            if index and index % 10 == 0:
                # Avoid wrap/cursor reposition at the physical PTY width,
                # which can otherwise make an individual new glyph invisible
                # or outside the crop. Never execute the unfinished command.
                send_key(win32con.VK_ESCAPE)
                time.sleep(0.18)
            before = frame().astype(np.int16)
            started = time.perf_counter()
            send_key(ord(alphabet[index % len(alphabet)]))
            observed = None
            changed_pixels = 0
            max_changed_pixels = 0
            while time.perf_counter() - started < 1.5:
                new = frame().astype(np.int16)
                count = int((np.max(np.abs(new - before), axis=2) > 25).sum())
                max_changed_pixels = max(max_changed_pixels, count)
                if count >= difference_threshold:
                    changed_pixels = count
                    observed = (time.perf_counter() - started) * 1000
                    break
                time.sleep(0.004)
            if observed is None:
                raise RuntimeError(
                    f"No visible update after ASCII sample {index + 1}; "
                    f"best_changed_pixels={max_changed_pixels} "
                    f"threshold={difference_threshold} "
                    f"prior_ms={[round(item, 1) for item in observations]}"
                )
            observations.append(observed)
            time.sleep(0.15)

        send_key(win32con.VK_ESCAPE)

        result = {
            "metric": "isolated-native-keydown-to-first-visible-pane-pixel-change-ms",
            "pane": pane,
            "samples": len(observations),
            "p50_ms": round(percentile(observations, 0.50), 2),
            "p95_ms": round(percentile(observations, 0.95), 2),
            "min_ms": round(min(observations), 2),
            "max_ms": round(max(observations), 2),
            "median_capture_ms": round(statistics.median(baseline_capture_ms), 2),
            "pixel_threshold": difference_threshold,
            "capture_quantized": True,
        }
        print("GPUI_VISUAL_LATENCY", json.dumps(result), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True, help="current isolated GPUI PID")
    parser.add_argument("--pane", choices=("primary", "secondary"), required=True)
    parser.add_argument("--samples", type=int, default=24)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows GUI test only")
    if not 3 <= args.samples <= 80:
        parser.error("--samples must be between 3 and 80")
    measure(args.pid, args.pane, args.samples, args.dry_run)


if __name__ == "__main__":
    main()
