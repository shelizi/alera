# Alera GPUI：Terminal 滿版／左右 Tab 對齊驗收（2026-11-12）

本輪基線 `37b6010ef`；工作分支 `integrate/gpui-central-diff-into-main-20261112`。使用者指出 GPUI Terminal 未像 Classic 一樣滿版，且左右切割的 Tab 列存在高低差。本輪只修這兩項**正常使用的 UI 缺口**，不將 T06 的輸入法／剪貼簿工作提前宣告完成。

## 根因與修正

- **左右 Tab 列：** 先前主 Pane 的 Tab 列位於分割容器上方，副 Pane 的 Tab 列卻在分割容器內，形成高度差。左右分割時改用同一個 header row 同時放主／副 Tab 列；上下分割則讓下方 Pane 保留它自己的列。只有單側存在 pinned tabs 時，也共用相同的額外 pinned row 高度。移除會在左右分割時重複顯示的副 Pane header，並保留右鍵、拖曳和分頁身分／焦點接線。
- **Terminal 不滿版：** `VtScreen` 與 viewport 預估原本限制為 240 欄 × 100 列，且 Host 成功 Resize 時不一定同步更新本地 VT 格線。調整為與 Host 支援範圍一致的最多 **512 欄 × 200 列**；Host 回報接受 Resize 後，只對身分完整、仍 live 的對應預覽同步本地格線，不建立或改變 PTY 所有權。
- **尺寸更新速度：** GPUI 的 Terminal Auto Resize 檢查由 1.5 秒 housekeeping 迴圈拆成單一每 400 ms 的檢查，避免新分頁長時間維持舊 80×24 預覽；沒有增加每個 Tab 各自的計時器。
- **畫面底色：** Primary VT grid、文字列、fallback scroll 及 Secondary 內容區補上同一個 VT 預設背景色，避免字元網格之外有未填滿／不同色的空帶。

涉及四個產品檔案：`desktop_gpui/src/main.rs`、`desktop_gpui/src/terminal.rs`、`desktop_gpui/src/vt_screen.rs`、`desktop_gpui/src/workbench_panes.rs`。

## 測試與結果

| 項目 | 結果 |
| --- | --- |
| TDD（新回歸） | 寬視窗大於 240 欄、Host accepted viewport 僅套用符合身分的 live 預覽、左右 pinned Tab 共用 row、VT 512 欄測試通過 |
| 原有尺寸邊界測試 | 更新三處仍期待 240×100 的舊上限，確認目前新上限 512×200 |
| GPUI Rust Library | `cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit` → **519 PASS、0 FAIL、1 FILTERED**。被排除的是先前長時間未完成、與本次 UI 修正無關的 Git History 搜尋測試，不宣稱全部 520 項通過 |
| Native Binary | `cargo test --offline --locked --bin alera-gpui-desktop` → **31 PASS、0 FAIL** |
| Windows EXE | `cargo build --offline --locked --bin alera-gpui-desktop` → PASS（最後背景與文字列底色修正後重新編譯） |
| 文字格式與差異 | `cargo fmt --all -- --check`、`git diff --check` → PASS |

## Windows 實機畫面／隔離 Host

- 使用專用 `desktop_gpui/target/smoke-pane-fullscreen-state-20261112` 與 `desktop_gpui/target/smoke-pane-fullscreen-runtime-20261112`，既有 `.gpui-center-diff-smoke` 作 Workspace；**不是正式 Classic Runtime**。
- 於 Windows 3440×1440 顯示器，重新開啟新的 GPUI EXE 並最大化。主、副 Pane 各有自己的既有 Terminal，**兩列 Tab 頂部與高度對齊**，沒有第二組副 Pane 列；可用高度由 Terminal 內容區及同色背景填滿。窗口正常切換時沒有修改其 Host Session 身分，沒有新建第三個 PTY。
- 隔離 Host `terminal list` 仍只有 **2 個 running Session**：左 `1dd9984f-c98a-46e2-93d1-f9e32e6ba00c`、右 `221669c2-30f4-45fe-a247-c2b1133ea634`。為了避免 GUI 焦點測試混入背景 LINE 視窗，使用 `terminal --no-start write/read` 獨立寫入與讀回 `FRAME_LEFT_FINAL_OK` 和 `FRAME_RIGHT_FINAL_OK`；左 Host 只有左標記、右 Host 只有右標記，無串流。
- **限制：** 這次最後的 GUI 點擊／文字輸入路徑尚未獲得可靠 Host 讀回，不能宣稱 T06 的 GUI 鍵盤、注音選字、Clipboard／快捷鍵通過；上述隔離 Host CLI 結果僅用於確認雙 PTY 未串流及正常執行。T06 仍為 NOW。
- GUI 已正常關閉；僅對本次專用 `smoke-pane-fullscreen-runtime-20261112` 執行 `runtime stop --force`，結果 `stopped=true`、停止前 `activeSessions=2`。未操作正式 Classic Host。
- 受保護 Git 測試 fixture 前後均是 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`，不重設、不清理、不提交；產品 worktree 的 `target/` 與 nested smoke 仍保留。

## 後續

- 本次僅標記 **U01 的 Tab／左右 Pane 對齊部分 PASS** 與 **T06 的 Terminal resize／滿版部分 PASS**，兩張工作單皆不因此全部結案。
- **NOW 仍是 T06**：正常 Windows GUI 的英文／繁中鍵盤、IME 組字、跨程式複製貼上、快捷鍵與 Shell，之後依原表接 T07。
- `supported=false` 保持不變；本輪不合併 `main`，不 Push、部署或 Release。
