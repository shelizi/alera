# Alera GPUI T06：Windows Terminal 剪貼簿、鍵盤與雙 Pane 正向驗收（2026-11-13）

起始 HEAD `086cc62d5`，工作分支 `integrate/gpui-central-diff-into-main-20261112`。本輪從已切換 Alacritty Terminal Core 的 GPUI 繼續 T06。修改僅限 GPUI Native Terminal 輸入事件、Pane Copy 及 TDD；**Classic Flutter、正式 Host／PTY 不改**。

## 實際修正

- 統一 `terminal_clipboard_shortcut`：**Ctrl+Shift+C → 複製目前 Pane**、**Ctrl+V／Ctrl+Shift+V → 貼上到目前 Pane**；普通 **Ctrl+C 仍傳送 PTY Ctrl-C**，不是 GUI 複製。
- 貼上使用 `terminal_clipboard_payload` 保持 Host 原本 **4 KiB** 單次寫入上限，拒絕空字串／NUL；由 Alacritty 的 `TermMode::BRACKETED_PASTE` 判斷 TUI 是否需使用 `ESC[200~…ESC[201~`。由現有非起動式 Host input queue／已觀測 Stream 發送，不新增命令列或 PTY。
- `copy_terminal_text_for_pane` 依照 **PaneSlot 和完整 Host tab context** 選擇資料來源；右側 Pane 的 Ctrl+Shift+C 和 GPUI Kit Context Menu Copy 不再固定複製主 Pane。已選取的 VT rows 保留原有 Host identity／輸出游標 stale 防護。
- 右側 VT rows 新增滑鼠選取、高亮與選取完成回呼；單一 `TerminalLineSelection` 繼續綁定精確 Host 身分，並將合法 rows 上限由 100 調整為與目前 preview 相同的 **200**。不對任何 Host PTY 發出選取或複製相關控制序列。
- 主窗格 VT／Transcript 兩個按鍵入口共用 `send_terminal_key_for_pane`，消除 Ctrl+Shift+C 的重複特殊處理；清除已停用的舊主窗格 copy wrapper。

## TDD 與 Windows 編譯

| 檢查 | 結果 |
| --- | --- |
| TDD RED | 新 `native_terminal_clipboard_shortcuts_and_bracketed_paste_are_safe` 在 helper 尚未建立時出現 E0432／E0425 編譯失敗；新增 helper 後 GREEN |
| TDD 新案例 | shortcut Ctrl+Shift+C、Ctrl+V、Ctrl+Shift+V、Ctrl+C 不被攔截、Alt／Platform 排除、bracketed paste 與超長／NUL 保護 PASS；新增 200-row Host snapshot selection 回歸 PASS |
| GPUI Library | `cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit`：**522 PASS、0 FAIL、1 FILTERED**。被排除的既有 Git History 長測本輪未執行，不得宣稱 523/523 |
| Native Binary | `cargo test --offline --locked --bin alera-gpui-desktop`：**31 PASS、0 FAIL** |
| Windows EXE | `cargo build --offline --locked --bin alera-gpui-desktop`：**PASS**，清理 unused wrapper 後重新編譯，未回報 warning |
| 格式 | `cargo fmt --all -- --check`、`git diff --check`：PASS |

## 隔離 Windows GUI 與 Host 正向驗收

- GUI state：`desktop_gpui/target/smoke-t06-keyboard-state-20261113`；Host Runtime：`desktop_gpui/target/smoke-t06-keyboard-runtime-20261113`；Workspace：既有 protected `.gpui-center-diff-smoke`。與 Classic／正式 Host 完全隔離。
- GPUI `+ → New Terminal` 新建主窗格 `cmd.exe`，使用 GUI 文字與 Enter 真實執行 `echo t06_keyboard_ready`。
- 儲存原本的純文字剪貼簿內容到 **Windows DPAPI CurrentUser 加密的短暫測試備份**，先後測試 `Ctrl+V` 與 `Ctrl+Shift+V`：Windows CMD 真實回應 `T06_CTRL_V_MARKER`、`T06_CTRL_SHIFT_V_MARKER`；最後以 DPAPI 還原原文並移除加密備份，不留下原始剪貼簿測試檔。
- 建立第二個 `cmd.exe`，右鍵 `Move to second pane`。右 Pane GUI 輸入 `echo T06_SECONDARY_COPY_ONLY`，`Ctrl+Shift+C` 後讀取 OS clipboard：**包含右側標記、沒有任何左側 CTRL_V／CTRL_SHIFT_V 標記**。切回左側並執行 `echo T06_PRIMARY_COPIED_OK`，左側 `Ctrl+Shift+C` 後 clipboard **有左側標記且沒有右側標記**。兩側複製對象正確。
- GUI 直接輸入並執行 `echo 繁體輸入測試`，畫面及 Host read 都正確含 `繁體輸入測試`；證實**提交後的 Unicode 文字**路徑可用，但此步不是注音候選字視窗測試。
- 在主 Terminal 文字行嘗試普通 `Ctrl+C` 後，再執行 `echo T06_CTRL_C_STILL_ALIVE`：Host 讀回正確，PTY 仍 running，Ctrl+C 沒有被 Copy shortcut 攔截。
- 主 Host handle：`071f357a-2da4-4e6a-abd4-9618dfb40729`；副 Host handle：`2033e4fb-396b-4083-8eca-69d9b76e8d45`。CLI `terminal --no-start list/read` 確認兩個 running Session，左側輸出含 CTRL_V／SHIFT_V／Unicode／CTRL_C 存活標記、沒有右側標記；右側只有右側標記，不混流。
- **注音候選字尚未通過**：Windows keyboard HKL 回報 `04040404`（繁中台灣鍵盤）；注音按鍵模擬 `su3` 未出現可確認的候選窗，文字進入 CMD 為 ASCII。不能判斷是 IME 當時處於英文模式、桌面輸入模擬方式還是 GPUI TSF 的組字介面，因此不可把它當作 Microsoft Bopomofo 實際選字 PASS，也尚不能斷言是產品故障；下一階段需在實際中文 IME 候選視窗與 commit 的端到端流程驗收。
- GUI 僅正常關閉本次 PID `40432`，只停專用 Host Runtime（停止前兩個 PTY）；受保護巢狀 fixture 原狀 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`，`target/` 未提交／清理。

## 後續與範圍

- **已通過：T06 常用鍵盤 Ctrl+C、Ctrl+V、Ctrl+Shift+V、Ctrl+Shift+C、雙 Pane Copy 與 Unicode 直接提交。**
- **NOW 仍是 T06**：真正 Windows 注音 IME 候選視窗及選字確認；副 Pane 滑鼠多列拖選在此輪有程式／純測試但尚未獨立 GUI 拖選驗收，不宣稱完成。若正常情境查到確實缺口，再修正。
- `supported=false` 不變；未合 `main`、Push、部署或 Release。
