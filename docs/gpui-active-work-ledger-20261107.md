# Alera GPUI 原生桌面：進行中修改與平行工作紀錄

更新日期：2026-11-07。權威程式位置：`desktop_gpui/`，主開發工作樹 `.worktrees/feat-gpui-desktop-20261107`。

**狀態：實驗性 / UNSUPPORTED。** 此紀錄區分程式已提交、headless 編譯測試、實機 GUI 尚未驗收。勿將模型測試等同 Windows 注音、選取、存檔、終端或完整 IDE parity 的實機證據。

## 已整理並併回 GPUI 開發分支

| 分類 | 提交 | 結果 |
| --- | --- | --- |
| Git | `7124d077b` | 唯讀 Git 變更清單與 patch/diff inspector；不開放寫入或合併交易 |
| 編輯器 WIP | `52d7736b2` | 每個編輯 tab 單獨關閉、dirty 二次確認、關閉最後 tab 清空 Workbench handle、切 tab 後重顯游標 |
| 無關格式 | `216694727` | 只有 Quick Open import 格式調整，與功能分開提交 |
| 檔案樹 | `5918566d8` | 安全、限額、懶展開的原生 Explorer Tree UI |
| 文字搜尋模型 | `2fbf6e55c` | 編輯器內 Find/Replace，循環與 Unicode grapheme 安全，替換沿用 Undo |
| 全專案內容搜尋模型 | `61978c4c1` | 唯讀限額搜尋，忽略產生目錄、symlink、binary/overlimit，回傳位置與 snippet |
| 未命名草稿復原 | `91a7f0955` / `9afb94b1d` | 多草稿各有識別，依 Workspace 復原、Save As/Discard 清理，GPUI 啟動與 Workspace 切換掛鉤 |
| 集成 | `dcad8c638` | 隔離合併並解決視窗初始化唯一衝突；已 fast-forward 回 `feat/gpui-desktop-20261107` |

來源：獨立平行 `feat/gpui-editor-find-20261107`、`feat/gpui-project-search-20261107`、`feat/gpui-scratch-recovery-20261107`；整合於 `integrate/gpui-parallel-20261107` 與 `integrate/gpui-consolidated-20261107`，均保留，無需再重複挑選源提交。

## 當次驗證

- Windows Rust headless：`cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`：**108 passed、0 failed、0 ignored**，基準 `dcad8c638`。
- Windows native：`cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline`：PASS。
- `cargo fmt --manifest-path desktop_gpui/Cargo.toml --all -- --check`：PASS；`git diff --cached --check`：PASS。
- 依賴 `proc-macro-error2 2.0.1` 有未來 Rust 相容性警示，但上述 check 退出碼為 0。

## 續作分工（不得誤列為已完成）

| 分支 | 責任檔案 | 完成標準 |
| --- | --- | --- |
| `feat/gpui-editor-find-ui-20261107` | `editor_ui.rs`（必要時新展示檔） | Ctrl/Cmd+F，IME 查詢，前後一筆、大小寫、取代與全部取代，dirty/Undo/草稿狀態安全；native check 與 headless 測試通過 |
| `feat/gpui-project-search-ui-20261107` | `main.rs`（必要時新展示檔） | 全專案搜尋面板，background query、結果 path:line:column、點擊開檔、切換 Workspace 取消舊結果；native check 與 headless 測試通過 |

兩條 UI 任務只有各自 worktree 的獨立提交可以合入；若有新 WIP，先審查、保留、切分提交。合入後重跑 **完整** headless + Windows native checks，保留最新 HEAD 與實際通過數字。

## 尚未關閉的產品 Gate

1. Windows 真實 GUI 長檔（601/5000 行）、滑鼠命中、拖曳選取、Undo/Redo、保存後 byte-level 核對：未正式驗收。
2. 微軟注音真實候選窗、composition、跨 DPI/視窗切換：未正式驗收。
3. Scratch 關閉重開、Save As 覆蓋拒絕、外部修改競態與草稿清除失敗之 GUI E2E：未正式驗收。
4. Git writer/merge、互動式 Host PTY 與跨 Workspace lease、Agent/MCP parity、macOS/Linux GUI、發布簽章：未完成。

**發行界線：** 本輪不合併正式 `main`、不推送、不打 tag、不 Release，不觸碰使用者現有安裝與 Runtime Host。
