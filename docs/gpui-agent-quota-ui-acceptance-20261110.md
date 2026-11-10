# GPUI Agent 額度 UI：Windows 實機對照（2026-11-10）

## 後續釐清：不是 Classic Host 故障，而是 GPUI 指向不同 Runtime 目錄

經進一步以 Windows 程序、Runtime 控制檔、監聽埠及 **不啟動 Host** 的 CLI 額度查詢交叉比對，已修正前面「Host 離線」的初步判斷：

- **Classic Host 仍正常存在**：實際 Runtime 目錄是 `%APPDATA%/dev.leynier/Alera/terminal_host`，本次檢查的程序 PID 19748（啟動於 11 月 7 日），其 `host.json` 指向 2050 埠；Owner PID 與程序相符，並可建立本機 TCP 連線。
- **錯誤來源**：GPUI 的 `default_runtime_dir()` 沿用獨立 Rust CLI 的 `%USERPROFILE%/.alera/runtime`。該目錄留有另一個 `runtime-host.json`（11 月 9 日），Owner PID 36180 已不存在、2019 埠無監聽，因此 `quota --no-start` 只對該目錄報 `Existing Alera Runtime Host is not running`。**不能據此宣稱 Classic Host 掛掉**。
- **真正的額度查詢驗證成功**：使用當前 worktree 編譯的新版 CLI、明確指定 Classic Runtime 目錄執行 `quota --no-start --json --runtime-dir <ClassicRuntime> snapshot`，成功獲得 7 種 Provider 快照：AGY、Codex、Devin、Grok 回傳 `ok`（AGY 五組額度），Claude、Kimi、Z.ai 含 `stale`。這些是 Host 回傳的狀態標記，並非推測。
- **GPUI 程式修復**：沒有 `ALERA_RUNTIME_DIR` 指定時，Windows 會優先選擇存在 `host.json` 的 Classic Runtime 目錄；明確指定仍優先，找不到 Classic 控制檔才回退獨立 Rust CLI 預設路徑。GPUI **不讀控制檔 token，也不啟動／重啟 Host**；由受控 CLI 完成認證。
- **測試**：`cargo test --offline --lib runtime_bridge::tests` 10/10 通過（含 Classic 與舊 standalone 路徑競合、使用者明確指定的覆蓋優先權）；全套 `cargo test --offline --lib --bin alera-gpui-desktop`：Library 390/390、Native UI 19/19 通過。Windows `cargo check --offline --bin alera-gpui-desktop` 通過。未更改執行中的 Classic Host／PTY；尚未以這次修正版 GPUI 完成真實帳號 UI 截圖驗收。

下方「Host 離線」為本輪修正前、**針對錯誤 Runtime 路徑**的當時觀察記錄，不再作為 Classic Host 全域狀態判定。

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

## 第三輪：狀態列真實排版驗收（隔離測試資料）

- 既有 Runtime Host 再次以 `cargo run --offline -p alera-cli -- quota --no-start --json snapshot` 讀取確認，仍回覆 `Existing Alera Runtime Host is not running`，本輪沒有啟動、覆蓋或重啟它。
- 補充明確**關閉預設**的 `desktop_gpui` Cargo feature `quota-visual-fixture`；只有「有此 feature」+「debug_assertions」+「ALERA_GPUI_QUOTA_VISUAL_FIXTURE 環境變數」同時符合，才會讀取編譯內建的固定 JSON 測試資料。正式 release 即使開此 feature 也不會啟用測試資料。所有數字純屬合成，絕不可當成任何真實帳號用量。
- 測試 JSON 放在 `desktop_gpui/tests/fixtures/agent-quotas-visual.json`：13 個帳號、覆蓋 10 種供應商；Claude 5H／Weekly／Fable，Codex 主副帳號，AGY 五組讀數，MiniMax G/V，OpenCode Go/Zen 與高低額度百分比。測試時兩個樣本帳號取消釘選；釘選的狀態列不顯示它們，但浮動總覽仍可見。
- **Windows 桌面實機觀察**：一般視窗及最大化尺寸下，狀態列直接顯示 Claude／Codex 多帳號、Kimi／Cursor／Antigravity 等的數值，窄視窗不擠掉分支選單，額度採水平捲動容納其餘項目。浮動清單的多帳號、Unpin／Pin、重新整理、Reset 描述與低額度警示色均可呈現；已測試其視覺位置及版面，沒有用 fake 資料接上正式 Host。
- 發現並修正原有 Codex、Grok SVG 的 `currentColor` 在 GPUI 深色主題下變成深色、難以辨識，保留原版路徑，僅將 CSS 顏色字串轉成淺色，並使用一次性靜態快取，不增加執行時檔案相依。
- 由於 Windows 遠端自動化滑鼠座標仍有不穩定情況，改讓**僅測試版**在啟動時展開浮動面板；這可驗證畫面，**不構成已手動點擊所有刷新／釘選按鈕的證據**。各 pin/排序邏輯另由單元及跨連線資料庫測試覆蓋。
- 以 `ALERA_GPUI_QUOTA_VISUAL_FIXTURE=agy` 直接打開 AGY 的隔離測試帳號詳情，在 Windows 畫面確認五組實際排版：`G·5H 97%`、`G·W 70%`、`C/G·5H 35%`、`C/G·W 6%`、`Ar·W 88%`，其中 6% 顯示低額度警示色；刷新／釘選鈕仍位於可見範圍。
- 進一步在測試版的狀態列及浮動面板加上 `DEMO quotas`／`SYNTHETIC TEST DATA` 提示，與實際讀取 Host 的正式版明確區分；此提示變更為測試專屬路徑，不會出現在預設建置。
- 全套 GPUI 測試 `cargo test --offline --lib --bin alera-gpui-desktop`：Library **388/388**、Native UI Binary **19/19** 全數通過，包括測試資料的 10 Provider／13 帳號完整性及 Codex SVG 深色主題修正。

## 第四輪：真正連接 Classic Host 的額度驗收（2026-11-10）

本輪在同一個 `integrate/gpui-p0p1p2-20261108` worktree，使用 `4fefa31bc` 的 GPUI Runtime 目錄選擇修正、同一 worktree 編譯的 `alera.exe`，**不啟用合成數據**，直接向仍執行的 Classic Host 讀取額度。

- Classic Host 位於 `%APPDATA%/dev.leynier/Alera/terminal_host`，可連線 `host.json` 所指向的 2050 埠。GPUI 視窗底部已實際顯示 Claude Code 與 Codex 真實 Host 快照（不保證每筆值都是最新）。
- `alera quota --no-start --json --runtime-dir <ClassicRuntime> config` 回傳啟用順序：`claude,codex,kimi,grok,agy,zai,devin`。對應 `snapshot` 回傳 **7 種 Provider**，AGY 的 `buckets` 有 **5 組**；Codex、Grok、AGY、Devin 回傳 `ok`，Claude 回傳 `stale`（OAuth 用量來源不可用），Kimi 與 Z.ai 為 `stale`（相關 API Key／額度來源未設定）。這些來源異常不是 GPUI 的連線問題。
- 只有具備 `quota-visual-fixture` Cargo feature **且是 debug 建置**時，設定 `ALERA_GPUI_QUOTA_LIVE_PREVIEW_OPEN=1` 可在測試視窗啟動時展開**真實 Host** 額度面板，與 `ALERA_GPUI_QUOTA_VISUAL_FIXTURE` 合成資料模式不同。預設及 release 不會自動展開；測試也沒有改變 Host 設定。
- 已實際看到面板中的 Claude、Codex、Kimi、Grok、AGY 用量與來源錯誤，額度列加入明確 `Stale`／`Unavailable` 標示，避免使用者誤把舊百分比視作即時；浮動面板在狀態列上方、內部項目有清單捲動。
- 當前 Classic Host 確實拒絕新版共用設定呼叫，回傳 `Unknown terminal host request: desktopSettings.get`。因此在舊 Host 執行期間，GPUI 的跨版本額度釘選**尚不可用**。已讓面板顯示 `Pin N/A` 與需更新 Host 的提示，取消無法完成的釘選點擊；若 Host 支援共用欄位，原 Rust CAS 釘選操作仍可使用。
- 遠端滑鼠測試的主要問題來自檢視器呈現圖片與 `coordinate_reference.image_width` 的縮放差異。改用與實際圖片大小一致的座標參考後，已可點開浮動面板，點擊面板內部時面板不再意外收起；惟**尚未證明** UI 的 `Refresh all` 與逐帳號刷新在所有 Provider 都可成功更新，亦未進行 Classic 與 GPUI 釘選跨視窗實際同步測試。
- Rust 全套回歸：`cargo test --offline --lib --bin alera-gpui-desktop`，Library **390/390**、Native UI **21/21** 通過；`cargo check --offline --bin alera-gpui-desktop --features quota-visual-fixture` 通過。這些測試不會啟動第二個 Host。

**安全界線：** Classic Host（及既有 PTY、Agent）、安裝中的 Classic、使用者既有其他 worktree 未被重啟、刪除或覆蓋；所有實機測試執行檔只保留在目前 worktree 暫存目錄並於驗收後清除。
