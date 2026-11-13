# Alera GPUI E06 — 雙 Editor Pane、草稿／Dirty Buffer 與重啟 Tab 順序

日期：2026-11-13（Asia/Taipei）
分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD：`884ee74af`。
狀態：**核心修正與 Windows Rust 回歸通過；真正 GUI 重啟／雙窗格存檔尚待驗收；`supported=false`。**

## 原功能盤點與此次發現

- 既有 `EditorTabs` 以不同 `EditorSession` 保存檔案或 scratch，Undo／Redo、Dirty Buffer 和獨立 draft store 已有回歸；跨 Pane 轉移搬移原實例，而非從磁碟重新讀取。
- Workbench V1 snapshot 保存左右 Pane 的 Editor key、active、pinned 與 Host 位置。啟動順序卻是 **先恢復 scratch，再開檔，再分派副 Pane**；先前只恢復 Pane 所屬關係，沒有恢復同 Pane 的 Tab 顯示順序或 Native Editor 的 Ctrl+Tab 順序。
- 這種情況會使重啟後未儲存文字仍在正確 Pane，但排序回到草稿／檔案的載入先後，與使用者先前拖曳的順序不同。

## 本輪變更

1. `workbench_panes.rs`：新增 `restore_editor_order`，以已還原的真實 editor 身分依快照重排各 Pane 的 Editor subsequence；忽略過期／重複 key，原有 Host preview 槽位與 active 身分保持不變。
2. `editor_tabs.rs`：新增 `restore_order_by_instances`，使用原本 `move_tab` 在 Native Editor 中排序，保留 EditorSession、Scratch ID、Dirty Buffer、Undo/Redo 與目前選取。
3. `editor_ui.rs`：接上 Native Editor reorder+notify，鍵盤 Ctrl+Tab 順序與顯示的工作區 Tab 順序相同。
4. `main.rs`：在 Startup 恢復真實文件 owner，並轉移至原 Pane 後，根據儲存的 Editor key 同步 mixed strip 及各 Pane 的 Native Editor Tab 順序。既有三重身分 Host/PTY 還原與安全轉移未改。

## Windows TDD／回歸證據

| 項目 | 結果 |
| --- | --- |
| RED | 先新增兩個還原排序測試，因缺少 `restore_editor_order`／`restore_order_by_instances` 編譯失敗（另修正測試 Undo 的 bool 斷言） |
| GREEN | 新測試 `restored_editor_order_handles_scratch_files_and_preserves_host_slots`、`restored_native_editor_order_preserves_active_buffer_and_undo` 均通過，`restored_` 篩選共 **7/7 PASS** |
| GPUI Library | `cargo test --manifest-path desktop_gpui/Cargo.toml --lib`：**560 passed／0 failed／1 ignored**（需隔離 Host 的測試未執行） |
| GPUI Native Binary | `cargo test --manifest-path desktop_gpui/Cargo.toml --bin alera-gpui-desktop`：**37/37 PASS** |
| Windows EXE | `cargo build --manifest-path desktop_gpui/Cargo.toml` **PASS** |
| 格式／差異 | `cargo fmt --manifest-path desktop_gpui/Cargo.toml -- --check`、`git diff --check` **PASS** |

相關既有測試涵蓋：實際檔案內容與磁碟衝突檢查、Unicode Undo/Redo、跨 Pane `EditorSession` 搬移、草稿 Identity 持久化、重啟後 scratch 恢復、Save As 路徑防護，以及 E05 同名跨 Worktree 開檔。

## 尚未宣稱通過的實際 GUI 操作

- 使用**可拋棄 Workspace／獨立 GPUI State**，左右 Pane 各開一份不同的 UTF-8 檔案；兩側分別編輯、Undo／Redo、`Ctrl+S` 並由磁碟讀回。
- 左右各新增、修改 scratch；將草稿／檔案 Tab 交錯排序，記錄各 Pane active/pinned。正常關閉 GUI、以相同 State 與 Workspace 重開，核對 Tab 順序、所屬 Pane、焦點、未儲存草稿內容與 Save As。
- 確認兩 Pane 與混合 Host 分頁並存時仍不串流、不丟 Dirty、沒有重複文件或錯誤儲存目標。
- 本輪 Windows 桌面前景有其他程式正在使用，沒有搶佔操作；上列仍是 **GUI 待驗，不是 PASS**。按照功能完整優先的既定隊列，E06 核心整合後進入 G01，不以尚未進行的 GUI 操作冒充完成。

## 範圍與發布狀態

- 只修改本 GPUI 整合分支；未修改 `main`，沒有 Push、部署或 Release。`supported=false` 保持。
- 受保護 `.gpui-center-diff-smoke/`、`desktop_gpui/target/` 未重設或清理；正式 Classic Host、使用者檔案及 Provider 帳號不屬本輪測試。
- 本項維持 **INTEGRATED／待 GUI 驗收**；後續追蹤檔的 `NOW` 前進 **G01 Source Control**。
