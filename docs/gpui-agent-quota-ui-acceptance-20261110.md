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

## 第二輪補齊：Classic 額度列對齊（2026-11-10）

這一輪仍使用原 GPUI 整合 worktree，不啟動／替換 Runtime Host，也不修改安裝中的 Classic。

### 已實作

1. **原版 Provider 圖示**：GPUI 直接編入既有 Classic 的十種 Agent Logo（SVG／PNG），避免執行期依賴 Flutter 素材目錄；狀態列及總覽均使用。已通過 Windows 編譯，**由於 Host 離線，尚無法在真實多 Provider 畫面逐一視覺驗證**。
2. **共用釘選設定**：由 Rust Core Runtime Store（`agents.quotaUnpinnedKeys`）管理，Classic 首次啟動將既有 Drift 釘選選項 seed-only 遷移，GPUI 使用同一份 Host CAS 欄位級讀寫。釘選鍵遵守 Classic 的 `claude:<account>`、`codex`／`codex:<account>`、`opencode:<account>` 等格式。舊 Host 未支援該欄位時，Classic 保留本機運作；GPUI 則拒絕離線寫入，避免雙份設定不一致。
3. **排序／顯示**：GPUI 的狀態列僅顯示已釘選帳號，總覽顯示全部；CLI 用 `quota --no-start config` 向既有 Host 讀取排序，**只回傳**白名單的 enabled provider IDs，不將 Host 的環境參數／Key 交給 GPUI。Host 未連線時使用 Classic 預設排序；Host 明確停用全部 Provider 時保持空清單。
4. **Classic 式浮動面板**：不再使用中央對話框，改由狀態列懸停／點擊開啟上方浮動總覽；有各帳號釘選、單項刷新、全部刷新與關閉操作，點擊其他區域可收起。

### 實際驗證

- Rust Core `desktop_settings_contract` 釘選鍵驗證：通過。
- Rust Core `runtime` 功能開啟後，兩個 RuntimeStore 連線測試：Classic seed、GPUI CAS 修改、Classic 讀回、stale CAS 拒絕、重啟保留與其他欄位不變，**1/1 通過**。
- GPUI 的帳號釘選鍵及排序單元測試：**1/1 通過**；錯誤／stale 文案 GUI 測試：**2/2 通過**。
- CLI 的 `quota` 操作及 secret-free 配置投影測試：**3/3 通過**。
- Flutter 的 RuntimeSettingsRepository 測試：**16/16 通過**，含新版 Host 釘選遷移、舊版 Host 的本機回退；Dart analyzer 無問題。
- GPUI Rust Library 全套回歸 **387/387 通過**；包含修正共享設定快照對新增 pin 欄位的驗證，並接受舊版 Host 未提供此可選欄位的相容模式。Windows GPUI `cargo check --offline --bin alera-gpui-desktop` 通過。
- **Windows 3840×2160 桌面實機**：GPUI 新版浮動面板能從狀態列上方開啟；點面板外可關閉。原版一開始因離線文案太長而擠掉關閉鈕，已拆成「標題 + 刷新 + 關閉」及「狀態訊息」兩列並重新編譯；重新實測後，關閉鈕可見且實際點擊有效。

### 尚未驗收／範圍外

- 本機既有 Runtime Host 仍離線：`alera quota --no-start --json snapshot` 回傳 `Existing Alera Runtime Host is not running`。**無法證明**真實額度、AGY 五組資料、跨視窗即時釘選同步、個別帳號刷新與多 Provider 圖示的最終視覺效果。
- Classic 的「Open Usage」完整統計、Codex 重置等擴充操作不是本輪額度狀態列對齊範圍，尚未宣稱對齊。
- 所有測試執行檔只暫存在 worktree，測試後清除；正式 Classic／Runtime Host／main 分支無變動。
