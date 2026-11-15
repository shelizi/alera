# Alera GPUI G04 — Git Graph 分支比較：開啟真正唯讀原檔

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD：`e12e8b6f5`。
狀態：**核心已接線、Windows Library／Native 回歸通過；實際 GUI 點擊待驗；Windows EXE 與格式驗證通過。`supported=false`。**

## 現有能力盤點

- Git Graph 已有 Local／Remote／All／Tags 清單、Refs 搜尋／最愛、Commit 搜尋與分頁、拓撲線與 Commit 預覽；右鍵選單包含雙方向分支 Compare、Worktree、Fetch／Push 等入口。
- 分支比較底層 `compare_branch_files` 使用當前 HEAD 與選取分支的 merge-base、保存 OID／方向及變更檔案；`diff_compared_file` 顯示唯讀文字 Patch，不切換分支，也不讀入未提交的工作樹內容。
- **發現正常 UX 缺口**：比較檔案清單的 Original 圖示僅呼叫 `preview_git_compared_file(index, true)`，只把 merge-base 原檔文字塞到右側 Diff 預覽；沒有建立 Native 可切換、不可編輯的 Git Original Tab。這與「比較分支可開原檔」的常用操作不一致。

## 本輪修正

1. `git_branch_compare.rs` 新增 `open_compared_original_snapshot`，由比較時捕捉的 **merge-base Commit ID + exact old/new path** 讀取原始 committed blob，組成 `GitOriginalSnapshot`。僅接受安全 Git 相對路徑、有效 40 字元 OID、存在的文字 Blob；二進位、無原檔的新檔案、超過 32 KiB 的資料明確拒絕。絕不使用目前磁碟檔案內容。
2. `git_inspect.rs` 的 immutable Git original 增加可選 `revision_label`；普通 Source Control 原檔仍顯示 HEAD／INDEX，比較分支的原檔則標為 `MERGE BASE <oid>`，避免誤標為目前 HEAD。
3. `git_original_tabs.rs` 的描述／標題尊重 revision label，分頁去重額外比對 revision label。即使同一路徑、相同 Blob OID，merge-base 與普通 HEAD 原檔仍是不同的唯讀 Tab，不混用。
4. `main.rs` 的 Graph compare Original 按鈕改接 `open_git_compared_original_tab`。背景讀取後再檢查 Workspace generation、原檔 request、比較方向、分支、目標及 merge-base OID，避免使用者選到別的比較後仍打開舊原檔。成功後顯示 Native GitOriginalTabs 的唯讀 Editor 視圖，不允許直接 Save／Save As；Copy to Scratch 是原有刻意明示的可編輯分支。

## TDD／Windows 回歸

| 驗證 | 實際結果 |
| --- | --- |
| TDD RED | 先在 Git Graph 比較與 immutable tab 測試中要求 `open_compared_original_snapshot` 與 `revision_label`，原始碼 E0425／E0609 編譯失敗，確認功能不存在 |
| GREEN 1 | `g04_merge_base_tab_is_distinct_from_head_original_with_same_blob` **1/1 PASS**，確認同一 Blob 的 merge-base Tab 與 HEAD Tab 身分分離，標題／描述正確 |
| GREEN 2 | 擴充既有 `directional_comparison_only_reads_committed_tree_contents` **1/1 PASS**，真臨時 Git Repo 的分支雙向比較僅讀 committed 原檔、不取未提交磁碟內容，並能安裝至 immutable tab |
| 全量 GPUI Library | **566 PASS／0 FAIL／1 ignored**；隔離 Runtime Host 測試未執行 |
| Native Binary | **41/41 PASS**（含原 Git Original／Diff UI 回歸） |
| Windows EXE／格式 | Windows EXE Build、`cargo fmt --check`、`git diff --check` 全部 **PASS** |

## 尚待真正 Windows GUI 驗收

- 隔離 Repo 同時具有 main／topic 兩個已提交的分支版本，Graph 選擇 topic、選擇比較方向、點改動檔案 Original；應出現 `MERGE BASE` 唯讀 Tab、可切換與關閉，不變成 Working Tree Editor。
- 比較檔案按 Diff 則仍是既有唯讀差異預覽；再切換 Ref／方向並開原檔，不能把舊比較內容誤當新的。對新增檔案沒有 merge-base 原檔時，應出現清楚拒絕訊息。
- Windows 真實滑鼠／鍵盤的 Graph Ref 搜尋／選擇與 Commit 右鍵完整 Classic parity 尚待驗，不因 Rust 測試就標記 GUI DONE。

## 安全、狀態與下一步

- 此次只變更 GPUI 整合分支；不合 `main`、不 Push、部署或 Release。受保護 smoke fixture 與 `desktop_gpui/target/` 仍保留。
- G04 核心狀態 **INTEGRATED／待 GUI 驗收**；依功能完整優先的固定隊列，下一項 **NOW G05：指定分支 Pull／Push、Merge 到目前分支但不 Checkout**。
