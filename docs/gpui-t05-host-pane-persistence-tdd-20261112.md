# T05：GPUI Host Pane 位置持久化與 Tab 操作（2026-11-12）

工作分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD `38c93f54b`。**T05A 持久化缺口已修復並通過實機；T05B 普通 Tab 操作已有部分實測，真正 Agent 跨窗格和乾淨 Preview 的 Close Others 仍待完成，因此 T05 整項維持部分完成／NOW。** `supported=false` 不變。

## 問題與實作

- 原本 TerminalLayout 會保存 Host Session／Workspace／Tab 身分並回復預覽，但 SavedWorkbenchLayoutV1 僅保存 Editor keys，造成主／副 Pane 的 Host 分配在 GPUI 重開後退回單 Pane。
- `desktop_gpui/src/workbench_layout_store.rs` 為 V1 快照增加可選 `primary_hosts`、`secondary_hosts`、`primary_active_host`、`secondary_active_host`，每筆只能保存有限長度的 Host handle／workspace_id／tab_id 三重身分；去重並限制筆數，舊 V1 JSON 無欄位時仍可讀，沒有自動執行能力。
- `desktop_gpui/src/workbench_panes.rs` 新增 `restore_live_host_positions`：只可移動**已在 WorkbenchPaneLayout 中的預覽**，且需有認證後的 Host `terminal list` 回報 `running=true` 及完全一致三重身分。重用 handle、不同 Tab ID、停止 Session 一律不能放回舊位置；不呼叫 Host create／attach／stop。
- `desktop_gpui/src/main.rs` 儲存兩窗格 Host 位置與各自 active Host。首次 render 早於異步 Host 清單，所以必須等待原 TerminalLayout 預覽還原，再安全套用 Pane 配置，避免先消耗一次性快照。既有 Editor 還原與 pins 繼續維持原路徑。

## TDD／建置

| 項目 | 結果 |
| --- | --- |
| TDD RED | 加入 V1 Host 三重身分 round-trip 與 authenticated running pane restore 回歸，編譯因缺少 `SavedWorkbenchHostTab`／新欄位失敗 |
| GREEN | `saved_host_pane_triples_are_bounded_deduplicated_and_backward_compatible` 及 `restored_host_pane_positions_require_running_exact_three_part_identity` 皆 PASS；含空值、過長、重複欄位及舊快照 |
| GPUI Rust Library | 最終 `cargo test --offline --locked --lib` **515/515 PASS**，0 failed |
| Native binary 測試 | `cargo test --offline --locked --bin alera-gpui-desktop` **31/31 PASS**，0 failed |
| Windows 建置 | `cargo build --offline --locked --bin alera-gpui-desktop` PASS，使用此 EXE 實機測試 |
| 格式及 Git 差異 | `cargo fmt --all -- --check`、`git diff --check` PASS |

## Windows 真實 GUI／隔離 Host

- 專用 Runtime `desktop_gpui/target/smoke-terminal-t05-runtime-20261112`、專用 GPUI State `desktop_gpui/target/smoke-terminal-t05-state-20261112`，僅使用現存巢狀 `.gpui-center-diff-smoke` 作 Workspace；不接觸 Classic Host。
- GUI 左側 `+ → New Terminal` 建立 A，第二個 `+ → New Terminal` 建立 B，從 B 右鍵 `Move to second pane`；新快照經讀檔證實 `split_enabled=true`、左右各一組完整 Host triple。
- `Alt+F4` 關閉 GUI 後用相同 state/runtime 重開，**左右雙 Pane 確實重現，各自有原 Terminal 與既有輸出**。隔離 Host `terminal list` 前後維持**兩個**執行中的原 Session，未多創建 PTY。
- 在兩 Pane 經 GUI 送入 `echo t05leftgood`／`echo t05rightgood`，獨立 `terminal read` 對照確認各自回覆正確、沒有混入另一側；Session 身分不變。
- 新增副 Pane scratch Editor（純 GPUI 測試草稿），再從副 Pane 新增第三個 Terminal C。右鍵 `Pin Tab`／選單顯示 `Unpin Tab`，證實 pins 可正常切換。從 scratch Editor 右鍵 `Close to right`，成功只關 C 的預覽，原 Host PTY 沒有因此終止。
- 從主 Pane 的 Terminal chip 實際拖曳到副 Pane 的 Terminal chip，主 Pane 預覽空、右側多一個 Host Tab；再由副 Pane 右鍵 `Move to first pane` 回原主 Pane，身份與已存在 PTY 不變。
- 對已 Pin 的 Terminal 執行 `Close others`，遇到本測試新增但未儲存的 scratch Editor，正常顯示 `Unsaved editor changes: Save or use Discard twice`，沒有直接捨棄草稿。這確認保護路徑，但**不將乾淨 Editor／Agent 的 Close Others 正向路徑標成已通過**。

## 非預期更新提示／後續工作

- 嘗試從主 Pane `+ → Codex` 作真正 Agent Tab 操作時，Codex 的 CLI 版本更新提示被 GUI 測試操作誤選為「Update now」。立即透過該**隔離測試 Terminal** 輸入 `Ctrl+C` 中止 npm 安裝。之後於同機以 `codex --version` 得到 `codex-cli 0.167.0`，與啟動前相同，未完成 CLI 升版。僅確認版本未變，**不能據此宣稱 npm 全域檔案未被部分觸及**；本輪沒有繼續操作該安裝程序，沒有擅自重裝／回復套件。此項不作 Agent 正向驗收。
- **T05B 待驗收：** 真正已登入 Agent 的主／副 Pane 移動與拖曳；乾淨 Terminal／Editor 的 `Close Others`／`Close to right`；需要時確認分割方向與跨窗格混排的正常行為。沿用已驗證的 Host 身分與安全檢查，不再讓 CLI 安裝提示進入測試範圍。
- GUI 已正常關閉；**只**停止 `smoke-terminal-t05-runtime-20261112` 隔離 Host（`stopped=true`，停止前共 4 個測試 Session），再查 `runtime status` 得 `running=false`、`host=null`。巢狀 Git fixture 的狀態前後均為原本 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`，沒有重設、清除、提交測試資料。
- 本次不合併 `main`、不 Push、不部署、不 Release；`supported=false` 維持。
