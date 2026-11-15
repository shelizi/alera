# Alera GPUI：VT100 → Alacritty Terminal Core 遷移（2026-11-12）

工作分支：`integrate/gpui-central-diff-into-main-20261112`。起始 HEAD：`2c93569c5`。本輪僅改 GPUI 的 Terminal **ANSI／VT 解析和畫面格線引擎**，保留 `VtScreen` 上層介面、GPUI StyledRow 渲染、Host-owned PTY、Session 身分及 Classic Flutter。**Windows Terminal 核心替換已通過正常流程驗收；T06 整體仍部分完成。** `supported=false` 不變。

## 依賴與實作

- `desktop_gpui/Cargo.toml`／`Cargo.lock` 將 `vt100 = 0.18.1` 完整移除，改為 `alacritty_terminal = 0.26.0`；Cargo 已成功下載、鎖定並編譯。`rg vt100 desktop_gpui/{Cargo.toml,Cargo.lock,src}` 無殘留。
- `desktop_gpui/src/vt_screen.rs` 保留對外 `VtScreen::{new,feed,reset,contents,styled_rows,cursor,alternate_screen,dimensions,set_preview_size}` 和 `CellStyle/StyledRow`。內部改用 `alacritty_terminal::Term<VoidListener>` 的 grid 與 `alacritty_terminal::vte::ansi::Processor`，由 GPUI 將格線轉成 RGB／UTF-8 styled spans。引擎的替代畫面、游標顯示、顏色、寬字元及 Combining marks 由 Alacritty 管理。
- ConPTY `CSI 8;rows;cols t` 的分段 Resize probe 繼續保留，跟隨已接收到的 Host 尺寸同步**本地預覽**，不會從此解析器自動建立或控制 PTY。單一預覽尺寸界限保持 5–200 列、20–512 欄，scrollback 限制仍是 240 行。
- 安全邊界：Alacritty Config 設為 `Osc52::Disabled`，避免 Host transcript 導致 GPUI clipboard 讀寫；`VoidListener` 不處理終端內要求的 Host 事件。`VtScreen` 同時暴露 `bracketed_paste`／`application_cursor_keys` 引擎模式查詢，方便後續 T06 鍵盤接線，但**尚未宣稱 T06 鍵盤與 IME 由此修復**。
- 沒有更動 Classic 的 `third_party/xterm`、Rust Core、正式 Host CLI、Hook、Agent 設定或使用者帳號資料；也不會因新引擎自動讓 Classic 與 GPUI 配色完全相同。

## 測試證據

| 項目 | 本輪結果 |
| --- | --- |
| Alacritty Core 專項 | `cargo test --offline --locked --lib vt_screen::tests`：**13／13 PASS**，包含新測試 `terminal_mode_switches_are_backed_by_live_alacritty_state`。既有 ANSI 16／256／truecolor、粗體／反白／游標、CJK／Emoji／Combining、OSC52、不完整 ANSI、alternate screen、ConPTY Resize 與 512×200 界限都通過 |
| 完整 GPUI Library | `cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit`：**520 PASS、0 FAIL、1 FILTERED**。被排除的 Git History 長時間測試並未於本輪通過，不可宣稱 521／521 |
| GPUI Native Binary | `cargo test --offline --locked --bin alera-gpui-desktop`：**31／31 PASS** |
| Windows EXE | `cargo build --offline --locked --bin alera-gpui-desktop`：**PASS**，以此建置檔啟動隔離 GUI |
| 來源格式與差異 | `cargo fmt --all -- --check` PASS，`git diff --check` PASS（提交前檢查） |

本輪先新增 Alacritty mode 的回歸測試；首次 Windows 冷編譯依賴時超過工作命令時限，因此**不能把那次當作已證實的 RED 失敗**。切換核心後專項 13／13 GREEN、Library 520 PASS；沒有偷改舊 ANSI 的正向測試以迎合新引擎。

## Windows 實機驗收

- 使用專用 GUI state：`desktop_gpui/target/smoke-alacritty-state-20261112`；專用 Host Runtime：`desktop_gpui/target/smoke-alacritty-runtime-20261112`。Workspace 只選現有、不可清理的巢狀 `.gpui-center-diff-smoke` Git fixture。
- GUI `+ → New Terminal` 建立真正的 `cmd.exe`，實際從 GPUI 輸入 `echo alacritty_shell_ok`，畫面正確出現輸出與下一次 prompt；Host `terminal read` 也包含該 marker。
- GUI `+ → Claude Code` 啟動真實 **Claude Code v2.1.322** 的 ANSI/TUI 歡迎畫面、顏色和互動輸入列，沒有發送遠端問題或觸發 CLI 更新。由原 Agent Tab 右鍵 `Move to second pane`，實機確認左側 Shell 與右側 Claude TUI 同時可見。
- Host 身分驗證：Shell handle `8011d545-1346-4712-967e-b750eed979be`／Tab `316c6b2d-ad53-47f4-8a55-d8cbcef89db6`；Claude handle／Tab `0fcd82b6-76fe-4b8c-833a-7971b4a47e5c`。兩者均 `running=true`，各自的 `terminal read` 只包含自己的標記（Shell 的 `alacritty_shell_ok`、Claude 的 `Claude Code`），沒有重建第三個 Session 或資料串流。
- 此隔離 Runtime 不提供正式 Hook 元資料，因此 GUI 的 Agent chip 可顯示為通用 Terminal；**TUI 確實是 Claude**，但 Hook 四態／Provider metadata 仍在 H01–H03 的其他工作項目。
- GUI 使用 Windows 正常關閉；只對 `smoke-alacritty-runtime-20261112` 執行 `runtime stop --force`（返回 `stopped=true`、停止前兩個 active Session），再確認專用 `runtime status` 為 `running=false`、`host=null`。
- Protected fixture 在實測前後 `git status --short` 都是既有 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`；不清除、不還原、不提交。`desktop_gpui/target/` 和 `.gpui-center-diff-smoke/` 保留原未追蹤狀態。

## 後續與限制

- **GPUI Core 替換可視為正常 Windows 流程完成**；但跨平台 Linux/macOS 原生 GUI、所有 Agent 的長時間 TUI 操作、kitty 鍵盤、滑鼠回報等進階控制仍未驗收。這些不可誤稱已完成。
- **NOW 維持 T06**：GPUI 自身英文／繁中 IME、剪貼簿、快捷鍵、Shell 的一般 UI 功能；Alacritty 的 mode 查詢可以供後續接線，但引擎替換**不會自動修復 Windows IME 或 GPUI 焦點**。下一項 T07 Agent 錯誤路徑保持排隊。
- 沒有合併 `main`、Push、部署或 Release；`supported=false`。
