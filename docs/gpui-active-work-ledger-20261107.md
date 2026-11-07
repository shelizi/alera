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
| 全專案內容搜尋 UI | `2a5531997` / `bf423497d` | 原生搜尋輸入、明確 Search、非同步掃描、至多 80 筆位置／摘要顯示、點擊安全開檔、工作區世代與請求序號避免舊回覆 |
| 編輯器 Find/Replace UI | `360b3e66e` / `46ca5947d` | Ctrl/Cmd+F、前後筆、大小寫、單筆／全部取代、兩個原生 IME 輸入、dirty/Undo/草稿保存及組字拒絕保護 |

來源：獨立平行 `feat/gpui-editor-find-20261107`、`feat/gpui-project-search-20261107`、`feat/gpui-scratch-recovery-20261107`；整合於 `integrate/gpui-parallel-20261107` 與 `integrate/gpui-consolidated-20261107`。新增兩條 UI 來源為 `feat/gpui-editor-find-ui-20261107` 和 `feat/gpui-project-search-ui-20261107`，整合於 `integrate/gpui-native-search-ui-20261107`，已快轉回原 GPUI 開發分支。保留相關分支供檢視，但無需重複挑選源提交。

## 當次驗證

- Windows Rust headless：`cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`：**108 passed、0 failed、0 ignored**，已於兩條搜尋 UI 合併後 `46ca5947d` 重新驗證。
- Windows native：`cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline`：**PASS**，同一搜尋 UI 整合分支。
- 編輯器 UI 另外以 `cargo test --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop editor_ui::editor_find_ui::tests --locked --offline` 驗證：**2 passed / 0 failed**（於原 UI 功能分支 `360b3e66e` 提交前）。這是獨立 binary tests，與 108 項 library 測試分開計數。
- 當次 `rustfmt --edition 2021 --check desktop_gpui/src/editor_ui.rs desktop_gpui/src/editor_find_ui.rs` 及 `rustfmt --edition 2021 --config skip_children=true --check desktop_gpui/src/main.rs`：PASS；`git show --check`：PASS。
- 快轉兩條 UI 後，已在原 `feat/gpui-desktop-20261107` 工作樹重新執行 `cargo fmt --manifest-path desktop_gpui/Cargo.toml --all -- --check`：**PASS（exit 0）**。另有局部 rustfmt／Git diff 檢查 PASS。
- 依賴 `proc-macro-error2 2.0.1` 有未來 Rust 相容性警示，但上述 check 退出碼為 0。

## UI 續作結果

| 分支 | 責任檔案 | 完成標準 |
| --- | --- | --- |
| `feat/gpui-editor-find-ui-20261107` | `editor_ui.rs`、`editor_find_ui.rs` | 已提交、已整合；108 library + 2 binary directed tests，原生 check 通過；未做實際 GUI／輸入法驗收 |
| `feat/gpui-project-search-ui-20261107` | `main.rs` | 已提交、已整合；背景搜尋、結果清單、安全路徑與跨工作區舊回覆防護；108 library 與原生 check 通過，GUI 仍未驗收 |

兩條 UI 任務均已合入。尚需完整 GUI、IME 與多尺寸／長檔接受測試，不得因 `cargo check` 成功就聲稱正式使用者流程已驗收。後續若有新的 WIP，先審查、保留、切分提交。

## 尚未關閉的產品 Gate

1. Windows 真實 GUI 長檔（601/5000 行）、滑鼠命中、拖曳選取、Undo/Redo、保存後 byte-level 核對：未正式驗收。
2. 微軟注音真實候選窗、composition、跨 DPI/視窗切換：未正式驗收。
3. Scratch 關閉重開、Save As 覆蓋拒絕、外部修改競態與草稿清除失敗之 GUI E2E：未正式驗收。
4. Git writer/merge、互動式 Host PTY 與跨 Workspace lease、Agent/MCP parity、macOS/Linux GUI、發布簽章：未完成。
5. 新增的 Search Contents 與 Find/Replace 工具列尚未在隔離 Windows GPUI 視窗進行真實滑鼠／鍵盤／注音輸入 E2E；目前僅確認編譯與相關模型/純函式測試。

**發行界線：** 本輪不合併正式 `main`、不推送、不打 tag、不 Release，不觸碰使用者現有安裝與 Runtime Host。
