# GPUI Agent 額度 UI：Windows 實機對照（2026-11-10）

## 範圍與環境

- 重用 `integrate/gpui-p0p1p2-20261108` worktree；對照機器同時開啟 Classic 與新編譯的 GPUI。
- GPUI、配套 `alera` CLI 由此 worktree 原始碼在 Windows 離線編譯，測試執行檔暫存於 worktree 的 `.quota-ui-test/`；不更動 Classic 安裝檔、工作階段或主分支。
- 桌面顯示器為 3840×2160，使用縮小截圖檢查布局；亦將 GPUI 最大化檢查底部狀態列。

## 直接觀察

| 檢查 | Classic | GPUI | 判定 |
| --- | --- | --- | --- |
| 額度入口 | 底部狀態列配額圖示；滑鼠移入／點擊會展開貼近狀態列的 hover card | 底部額度入口；點擊會開啟畫面中央對話框 | 不一致 |
| 列表外觀 | 供應商專用圖示、窄行、多組讀數、每列刷新與釘選控制 | 以供應商名稱與讀數為主的長條布局；對話框提供刷新 | 不一致 |
| 釘選 | Classic 依使用者設定顯示釘選 Provider／Profile | GPUI 目前未使用相同釘選偏好 | 缺口 |
| 額度總覽 | 已實際打開，看到 Claude Code、Codex、Kimi、Grok Build、Antigravity、Z.ai、Devin 等項目；AGY 顯示五組讀數 | 已實際打開，但 Host 不在線，僅能觀察錯誤／空白狀態 | 部分通過 |
| 重新整理 | Classic 清單有各項刷新與「開啟用量」 | 已實際按下 GPUI「Refresh all」，UI 未崩潰 | 部分通過 |
| 即時數據／多帳號 | Classic 畫面有百分比，但即時有效性無法在 Host 中斷時確認 | `alera quota --no-start --json snapshot` 回報 Host 未執行 | 阻塞，不能宣稱通過 |

## Host 阻塞證據

Classic 編輯區顯示：`Terminal host unavailable: Terminal host connection closed`、Host 已停止回應 liveness probes。新編譯 CLI 的唯讀 `quota --no-start --json snapshot` 回傳 `Existing Alera Runtime Host is not running`（exit 1）。`runtime-host.json` 雖存在，但不足以證明 Host 活著。**為避免影響 Classic 的既有 Terminal/PTY，這輪沒有啟動、重啟或替換正式 Host。**

因此尚不能驗證所有供應商真實百分比、AGY 五組動態更新、帳號切換、單帳號刷新與離線後重連。程式資料模型既有單元測試不能取代 UI 驗收。

## 本輪直接修正

1. GPUI 原本在**從未取得任何額度**且讀取失敗時，錯誤顯示「previous readings are stale」；此文案有誤，已改為要求連上既有 Runtime Host。保留**已有額度再刷新失敗**的 stale 提示。
2. 狀態列左側很長的 Host/Git 異常文字會吃掉額度列空間；已對左側狀態文字設置最大寬度，讓額度入口保有展示空間。
3. 新增兩項離線／stale 狀態文案回歸測試。

上述修正已重新編譯並**再次在 Windows GPUI 視窗實際開啟額度對話框**核對：
空資料時顯示 `Quota unavailable · connect to the existing Runtime Host`，不再誤稱存在過期讀數。
`cargo test --offline --bin alera-gpui-desktop quota_ui_tests`：2 passed、0 failed；`git diff --check` 通過。

## 後續驗收門檻

1. Runtime Host 恢復連線後，再同時觀察兩版：各 Provider 額度、AGY 五組讀數、Profile 名稱、reset 時間與資料新鮮度。
2. 實作並測試 Classic 的釘選／取消釘選、Provider 圖示、狀態列排列、hover card 或等價的低干擾浮動面板。
3. 由 GPUI 實際操作單一帳號刷新、全部刷新、列表溢出、窄視窗布局及 Host 中斷重連，確認不會錯誤標示數值為最新。

本文件描述**已實際看到的畫面與 CLI 回應**；未操作到的功能保留為「未驗收」，不是通過。
