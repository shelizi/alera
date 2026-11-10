# GPUI-01 visible-line hit, G0 local evidence

ticket: G1a library gate for current-viewport pointer hits. Live GUI cases stay blocked.
start_head: e40ae5393f5e82e8000ce0abcbcd8960e2e2b8e1
end_head: c8ccc79c1ec4003ac15259463195caf68b3f56a5
branch: feat/gpui-desktop-20261107
os: Windows
scope: `VisibleLineHitSet` decides pointer hits from the current visible rows only. The GPUI window forwards that decision. No second host, PTY, Git writer, or protocol change.
non-goals: Live mouse, scroll, Zhuyin, DPI, remote Actions, and making GPUI the supported desktop.

## Results

- `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked`: 84 passed, 0 failed, 0 ignored.
- `cargo fmt --check` and `git diff --check`: exit 0.
- `cargo check --all-targets --locked` and `cargo build --locked`: exit 0. Binary `D:\c\t\debug\alera-gpui-desktop.exe`.
- Isolated launches of that binary, PIDs 4544 and 30096, on `%TEMP%\alera-gpui-acceptance\workspace-A`. `ALERA_CLI_PATH` was `Z:\missing-gpui-test-cli.exe`. No `host.json` or `runtime-host.json`.
- Fixture SHA-256 before and after both launches:
  - `5000-lines.txt` `BDC11D891F86FFBE34A80BD8D32686BCE6F56DC1178E11185F063EF43BDF6665` (144999 bytes)
  - `601-lines.txt` `3F1A5B790BF80B5EFB960B7E71F554DDD1A4147B4E6218B2E2A417E24B604452` (17428 bytes)
  - `empty-and-crlf.txt` `3471C039C8D2E204E5C3E1B4731F6DB4AE0C11EBB387CC1FDECE1AFCB9A94495` (15 bytes)

## Not run

G1a-02 through G1a-10 live window actions are **BLOCKED**. This session does not inject global mouse or keyboard, so Edit, Ctrl+Home, Ctrl+End, click, drag, copy, undo, and save were not performed. A process-alive launch is not that acceptance. G1b IME, G1c OS save races, macOS/Linux GUI, and remote Actions remain **NOT RUN**. Merge, push, and release: NO.
