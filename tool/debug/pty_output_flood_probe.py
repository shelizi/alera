"""Generate exactly 6 MiB of PTY output, then signal completion by file.

This is strictly bounded: no threads, no sleep, no unbounded allocations.
"""
import pathlib
import sys

SIZE = 6 * 1024 * 1024
CHUNK = b"X" * 65536


def main():
    if len(sys.argv) != 2:
        raise SystemExit("expected one completion marker path")
    for _ in range(SIZE // len(CHUNK)):
        sys.stdout.buffer.write(CHUNK)
    sys.stdout.buffer.write(b"\r\nALERA_OFFLINE_FLOOD_DONE\r\n")
    sys.stdout.buffer.flush()
    pathlib.Path(sys.argv[1]).write_text("done", encoding="ascii")


if __name__ == "__main__":
    main()
