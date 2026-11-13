# GPUI U05｜額度狀態列與 Classic UI／UX 對齊

日期：2026-11-13（Asia/Taipei）；起始分支 HEAD `5698c981a`。
此為 U05 新增的 UI／UX 子項，不改變 T07 NOW 或全域 `supported=false`。

## 對照 Classic 的日常功能缺口與修正

| 功能 | Classic 行為 | 本輪 GPUI |
| --- | --- | --- |
| 更新頻率 | 每 5 分鐘自動更新 | `REFRESH_INTERVAL_SECS` 由 900 秒改 300 秒（來源仍為同一個 Host Quota Snapshot） |
| Status 顏色 | 剩餘低於 20% error、低於 50% warning、其餘 success | 使用 Classic 共用 palette `AGENT_BLOCKED/WAITING/DONE`，stale 以 faint 而非正常綠色；無有效百分比不虛構 |
| 額度列密度 | Provider 圖示＋必要帳號別名＋5H/W 等短讀數 | 移除每個 Chip 重複的完整 Provider 名稱；單帳號只顯示圖示，Claude/Codex 多帳號可辨識 Default/Work/Secondary；OpenCode Go／Zen 獨立辨識 |
| 額度總覽 | Gauge 圖示、Hover Card／Provider Account 明細 | 正式 Gauge SVG、Hover 及 Click 開明細；合成資料仍保留 `DEMO quotas` 警告，不假報真實額度 |
| 更新 | 狀態列直接刷新、明細個別刷新 | Status 新增約 22 px 的 Refresh 圖示按鈕，執行現有 `refresh_agent_quotas(true, None)`，不新增額度請求實作 |
| Overview | 380 px／480 px 面板 | 對齊尺寸，清單獨立縱向捲動，頂部 Refresh all／Close 不隨清單滾走；讀數可換行以避免五項 AGY 被裁切 |

## TDD 與 Windows 視覺驗收

- RED：`classic_status_bar_refreshes_five_minutes_and_uses_compact_account_labels` 因缺少 `status_bar_account_label` 而 E0425；GREEN：來源間隔 300 秒、Default/Work 與 OpenCode Go 標籤正確。
- `status_bar_remaining_colors_follow_classic_thresholds_without_stale_green`：19% error、20/49% warning、50% success，stale faint、error blocked、缺失值 faint，單元 PASS。
- 原先 10 種 Provider／13 個測試帳號，以及 AGY 5 組額度模型的回歸仍保留，不減少資料或隱藏用量。
- Windows GUI 以 **僅 Debug + Cargo `quota-visual-fixture` feature + `ALERA_GPUI_QUOTA_VISUAL_FIXTURE=agy`** 的三重明確允許條件使用合成資料，測試環境獨立 `ALERA_GPUI_STATE_DIR` 與不存在 Host 的 `ALERA_RUNTIME_DIR`。觀察 Claude/Codex 多帳號緊湊狀態列、滑出完整 Overview、捲動時 Refresh all／Close 固定，以及 AGY `G·5H 97%`、`G·W 70%`、`C/G·5H 35%`、`C/G·W 6%`、`Ar·W 88%` 全部可見。單擊底部直接刷新圖示後測試面板重載成 AGY 詳情；**不是**真實 Provider Refresh 成功證據。
- 測試使用受保護 fixture 專案僅顯示既有檔案，沒有做 Git 檔案操作，沒有登入任何 Provider、沒有保存新憑證。兩次隔離 GUI 都已關閉，測試資料位於被忽略的 `desktop_gpui/target/`，未清除。
- 完成此輪後重新執行**不帶 feature**的標準 Windows EXE 建置，確保一般 Debug／Release 不會誤顯示合成資料。

## 尚需驗收

- 本輪沒有接真實 Classic Host 的每個 Provider Refresh API 成功率、跨 Classic/GPUI 釘選同步及 Codex 重置等更進階動作；這些保留原額度驗收文件中的邊界，不因 UI 已對齊就宣稱全通過。
- U05 目前屬**正常 UI 合成數據視覺 PASS，真實 Host 行為待複核**。T07 仍 NOW，`supported=false`；不合併 main、不 Push、不部署、不 Release。
