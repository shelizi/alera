# Alera GPUI T02：Installed Agent CLI 偵測、刷新與 Classic 下拉選單對齊

> 日期：2026-11-12（Asia/Taipei）。工作分支：`integrate/gpui-central-diff-into-main-20261112`，本輪起始 HEAD `6ce141b79`。狀態：**完成（Windows 正常流程）**。`supported=false` 不變。

## Classic 對照與本次修正

- Classic `lib/src/features/workbench/presentation/workspace_workbench_new_tab_button.dart` 的 `+` 選單依序顯示 `New File`、`New Terminal`，再列出 `agentDisplayName(agentType)`；使用 `installedAgentClisProvider` 取得本機 CLI，而非以 Host Profile 是否存在判斷。
- Native `desktop_gpui/src/local_agent_detection.rs` 已經從共用 Rust `AGENT_DESCRIPTORS` 產生同一順序與名稱；既有 PATH/PATHEXT、override、僅 AGY 需要 `--version` 的偵測與背景掃描保留。主／副 Pane 皆共用 `kit_new_tab_menu`，啟動與 Host Session 流程不變。
- 本次確實修正的差異：原 GPUI 選單顯示 `New Agent · Codex` 等額外前綴，Classic 則顯示 `Codex`。現在 `InstalledAgentCli::new_tab_menu_label` 直接返回共用 descriptor 的 `display_name`，`native_popup_ui.rs` 不再加上額外前綴。
- Native 額外保留 `Installed Agent CLIs`、獨立的 `Host Agent Profiles`、`Rescan installed agents (reopen menu)` 與 Attach 選項；本輪未移除其他既有功能。

## TDD／檢查

| 驗證 | 結果 |
| --- | --- |
| RED | 新增 `installed_agent_new_tab_labels_match_classic_unprefixed_names_and_order` 後，Rust 編譯因缺少 `InstalledAgentCli::new_tab_menu_label`（E0599）失敗，證實測試先於實作 |
| GREEN | 實作直接採用 descriptor 顯示名稱、Native 下拉入口改用該名稱；新測試 **1/1 PASS** |
| 相關 GPUI Rust Library | `cargo test --offline --locked --lib` **513/513 PASS**，0 failed |
| 格式與差異 | `cargo fmt --all -- --check` PASS；`git diff --check` PASS（Windows CRLF 轉換提醒不影響結果） |
| Windows Native 建置 | `cargo build --offline --locked --bin alera-gpui-desktop` PASS；用此新 EXE 驗收 |

## Windows GUI 實際正向驗收

全部使用已存在的受保護 `.gpui-center-diff-smoke` 測試工作目錄，以及**獨立 GPUI state**。Runtime 指到已停止的 `desktop_gpui/target/smoke-terminal-t01-runtime-20261112-1900`（讀取不會啟動 Host），未使用正式 Classic Host。

**正常安裝情境**：獨立狀態 `desktop_gpui/target/smoke-agent-t02-positive-state-20261112`，用系統當前 PATH 啟動本分支新編譯 EXE。

- 主 Pane `+` 的本機清單依序顯示 `Codex`、`Claude Code`、`GitHub Copilot`、`Antigravity`、`Grok Build`、`Devin`；皆不帶 `New Agent ·`。選單仍有 `New File`、`New Terminal` 與分隔的 Host Session 入口。
- 從主 Pane 點選 `Rescan installed agents (reopen menu)` 並重新開啟，六個 Agent 的名稱與順序保持正確。
- 在不啟動 Host／PTY 的情況下，僅新增隔離狀態中的本地 Untitled scratch Tab 並使用 Pane Actions 移到第二 Pane；副 Pane 自己的 `+` 再次顯示相同六項、相同順序與名稱。沒有操作測試 fixture 的磁碟檔案。

**未安裝情境**：另一個獨立狀態 `desktop_gpui/target/smoke-agent-t02-empty-state-20261112`，僅將**該 GPUI 子程序**的 PATH 設為 `C:\Windows\System32;C:\Windows`，未改系統或使用者 PATH，也未解除安裝任何 Agent。

- 主 Pane `+` 顯示 `No installed Agent CLIs detected`，沒有假 Agent 選項，仍保留 `New File`、`New Terminal`。
- 點選重新掃描、關閉後再開選單，仍正確顯示空清單。因為此情境故意移除 PATH 的 Git，側欄顯示 `Git unavailable; program not found` 是隔離 PATH 測試限制，**不是**對一般 Git 功能的評估。

測試後兩個獨立 GPUI GUI 都已正常關閉。再查詢測試 Runtime 得到 `running=false`、`host=null`；保留巢狀受保護 Git fixture 的既有 `M center-diff-smoke.txt` 與 `?? center-diff-source-switch-untracked.txt`，並未重設、提交或刪除。

## 範圍與下一步

- T02 只覆蓋正常 Windows 自動偵測、手動重新掃描、找不到本機 CLI 時的明示，以及主／副 Pane 與 Classic 名稱排序一致。**沒有**真的啟動任一 Agent Provider；登入、真實 TUI 互動、Provider 錯誤訊息是 **T03 NOW**。
- 沒有模擬應用執行期間重新安裝 Agent 或變更 PATH 的外部程序；macOS/Linux 登入 Shell PATH 差異、Agent 圖示與其他 UI 視覺一致性，留待對應跨平台與 UI／UX 階段，不拿極端驗證阻塞 T02 的 Windows 日常功能。
- 未合併 `main`、未 Push、未部署、未發版；`supported=false` 保留。
