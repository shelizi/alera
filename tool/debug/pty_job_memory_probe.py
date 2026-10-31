"""Bounded Windows Job Object memory pressure probe (invoked inside test PTY).

Requests a single >Job-limit VirtualAlloc MEM_COMMIT|MEM_RESERVE without
touching pages; Windows should reject the allocation before consuming physical
memory. There is no retry, no unbounded loop and no system-wide OOM.
"""
import ctypes
import sys

SUCCESS = "ALERA_MEM_ALLOCATION_REJECTED"
UNEXPECTED = "ALERA_MEM_ALLOCATION_SUCCEEDED"


def main():
    if sys.platform != "win32" or len(sys.argv) != 2:
        raise SystemExit("Windows only; usage: pty_job_memory_probe.py ALLOCATION_MB")
    request_mb = int(sys.argv[1])
    if not 256 <= request_mb <= 512:
        raise SystemExit("memory probe allocation must be 256-512 MiB")
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.VirtualAlloc.argtypes = [
        ctypes.c_void_p, ctypes.c_size_t, ctypes.c_ulong, ctypes.c_ulong
    ]
    kernel32.VirtualAlloc.restype = ctypes.c_void_p
    kernel32.VirtualFree.argtypes = [
        ctypes.c_void_p, ctypes.c_size_t, ctypes.c_ulong
    ]
    kernel32.VirtualFree.restype = ctypes.c_int
    address = kernel32.VirtualAlloc(
        None, request_mb * 1024 * 1024, 0x1000 | 0x2000, 0x04
    )
    if not address:
        error = ctypes.get_last_error()
        print(f"{SUCCESS}: Win32Error={error}", flush=True)
        return 0
    kernel32.VirtualFree(address, 0, 0x8000)
    print(UNEXPECTED, flush=True)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
