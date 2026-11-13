# Alera GPUI G01 — Git Source Control 多選 Stage／Unstage

日期：2026-11-13（Asia/Taipei）
工作分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD：`27cb992b3`。
結論：**Git Index 正常流程及新缺口已修補，Windows Rust／Native 測試通過；Windows GUI 真實滑鼠與 Ctrl／Shift 多選仍待驗收，`supported=false` 不變。**

## 現有功能盤點

- Source Control 已有 `GitSnapshot::visible_rows`，將同一檔案的 Staged 與 Working Tree Changes 拆成獨立列，並顯示 Untracked／Conflicts。
- `git_sidebar_layout` 已有依區域／全部統合、Flat／Tree、檔名篩選與展開收合。GPUI `main.rs` 已有 Ctrl、Shift、多選上限 120、選取工具列 Stage／Unstage、右鍵選取同區域批次操作，成功後刷新 Git 狀態。
- `GitActions::stage_files` 與 `unstage_files` 原本採隔離 Git Index 的批次 API；本輪以真 Git Repo 的臨時測試重驗 Index／Worktree 邊界。

## 本次修復的兩項實際缺口

1. **Staged Deletion 批次 Unstage 被誤拒**：原本逐個要求 `index.get_path(path, 0)` 非空；但正常已暫存的刪除項目在 Index 已沒有該 path，GUI 因此一律收到「not staged」。改成逐項讀 `repo.status_file` 的 `INDEX_NEW／MODIFIED／DELETED／RENAMED／TYPECHANGE`，且拒絕 Conflicted／不再 staged 的過期列。在真正寫入 Index 前先對整個 selection 驗證；已有的 HEAD/unborn 路徑維持，對未選檔不修改。
2. **任一 Dirty Editor 全域阻擋 Stage／Unstage**：原先 `run_git_action` 不分操作，一律要求全工作區 Native Editor 先 Save。Stage／Unstage 是 index-only 操作，不需要更動 Editor 或工作樹，因此現在允許 Stage／Unstage 單筆及批次在 Dirty Editor 存在時執行，只操作磁碟上已儲存的內容；成功訊息明示未儲存 Editor 文字沒有被 Stage。Discard、SwitchBranch、Merge、Commit 等其餘既有安全保護沒有放寬。

## TDD 與測試

| 驗證 | 結果與界線 |
| --- | --- |
| G01 RED | 使用真正初始化的臨時 Git Repository，Stage 已追蹤刪除及修改、新檔，再混合 Unstage。修正前 `unstage_files` 回傳 `Selected file is not staged`，測試確實失敗 |
| G01 GREEN | `g01_batch_stage_unstage_includes_staged_deletion_and_preserves_worktree` PASS；Git Status 與 Index 類別返回正確，未選資料、磁碟 bytes、HEAD 不改 |
| 原子預檢 | `g01_unstage_many_rejects_unstaged_selected_row_without_index_change` PASS；混入未暫存路徑時整批拒絕，原 Index Tree、工作樹內容不動 |
| GPUI 動作分類 | `g01_source_control_action_tests::stage_and_unstage_are_index_only_but_discard_checkout_and_merge_are_not` PASS |
| Rust Library | **562 passed／0 failed／1 ignored**（需隔離 Host 的測試未執行） |
| Native Binary | **38/38 PASS** |
| Windows EXE／格式 | 最終建置與 `cargo fmt --check`、`git diff --check` 另在本輪收尾確認，不以測試通過替代建置證據 |

## 待正常 Windows GUI 驗收

- 在**專用臨時 Git Repo + 獨立 GPUI State**，建立 tracked modified、tracked deleted、untracked 各一筆，透過 GUI Changes／Untracked Ctrl／Shift 多選 Stage，確認工具列及各區域數目、Git Index 內容，然後從 Staged 多選 Unstage 並獨立讀回。
- 同一檔案同時 Staged + 另外的 Working Tree 未儲存更動時，確認兩列不混淆、只 Unstage 選取範圍；儲存前 Editor Dirty 的內容不可偷進 Index。
- 切換 Flat／Tree 和篩選時，多選不得偷偷包含已隱藏列；右鍵單筆／批次行為應與工具列一致。
- 本輪沒有取得上述 GPUI 滑鼠／鍵盤實際操作證據，**GUI 仍為待驗，不能標完成**。按已確立的「功能完整優先」策略，核心實作驗證完畢後 NOW 轉入 G02 中央 Git Diff。

## 安全與發布範圍

- 所有新增寫入測試皆使用 `tempfile` Repository；沒有操作正式專案的 Git Index 或 Worktree 文件。
- 保留原 `main`、User WIP、`.gpui-center-diff-smoke/` 和 `desktop_gpui/target/`，不自動清理；未 Push、部署或 Release。
- `supported=false` 不變。G01 狀態為 **INTEGRATED／待 GUI 驗收**；下一個 NOW 為 **G02**。
