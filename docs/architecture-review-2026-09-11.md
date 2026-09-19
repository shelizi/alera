# Alera 整體架構盤點與後續重構路線

日期：2026-09-11。程式碼基準：`90360f96fe4dc18025e9e16ce54fdb9a30c3cd29`。分支：`refactor/runtime-boundary`。本報告是全系統架構盤點，不是下一輪零碎拆檔清單。

## 1. 本輪完成事項與驗證邊界

退出流程的 application ownership 已由既有提交 `7fc71c45` 完成；接手時分支也已包含後續 runtime、diagnostics、resource-manager 和 orchestration 修改，沒有重做或重複提交它們。

本輪提交 `90360f96`，訊息為 `test(runtime-host): align status panel lifecycle contract`。修改僅限 `test/widget/runtime_host_status_panel_test.dart`：補齊 test fake 的 begin/cancel/commit quit 方法，並把 fake 拋出的舊 transport startup exception 改為 application 的 `RuntimeHostLifecycleStartupException`，移除舊 transport import。

實際先看到紅測：狀態面板預期顯示專用 startup 錯誤訊息，卻顯示一般 runtime action 錯誤。修正 fake 的介面契約後，原本的訊息與 error tone 斷言保持不變，以下七個測試檔合計 67/67 通過。這不是「所有 Alera 測試通過」。

```text
flutter test test/widget/runtime_host_status_panel_test.dart test/unit/diagnostics_providers_test.dart test/unit/runtime_host_lifecycle_service_test.dart test/unit/runtime_host_lifecycle_quit_test.dart test/unit/runtime_host_quit_planner_test.dart test/unit/app_window_lifecycle_coordinator_test.dart test/widget_test.dart
```

`runtime_architecture_guard` 與 `git diff --check` 通過。Widget smoke 仍出現 Drift multiple-database warning，應另查測試 fixture 與 bootstrap 的資料庫所有權；本輪沒有證據可以將它判定為正式環境資料毀損。

接手前已有 `rust/alera-core/src/runtime/orchestration_store_tests/v2_contract.rs` 未提交修改，本輪沒有修改、提交或宣稱驗證該補丁。沒有 merge、push、部署或重新打包。完成退出補丁後只進行盤點與文件整理，沒有擅自開始下一階段功能重構。

## 2. 結論

Alera 已有分層、窄介面、runtime authority、失敗恢復與大量測試，不適合推倒重寫。主要問題是：**有些責任已形成真實邊界，有些仍只是分散到多個檔案、共用同一個 state 或大型 owner；跨 desktop、Rust runtime、mobile、cloud/edge 的契約驗證也不均衡。**

下一階段應由「看到長函式就抽 coordinator」改成「按能力與所有權拆分」：先固定契約與自動化護欄，再處理 runtime 排程、資料操作一致性、Workbench 狀態與 terminal session ownership。小批次 TDD 仍保留，但每批必須朝可說明、可驗收的模組邊界前進。

## 3. 範圍與方法

盤點涵蓋 desktop Flutter、native FFI、Rust CLI/runtime、runtime storage/orchestration、mobile、cloud API、edge relay、共用 packages、CI、release/portable 與 landing/infra 的整合位置。採用 Git 追蹤檔案統計、import/export 靜態依賴分析、關鍵 owner/資料流原始碼閱讀及 workflow 檢查。

這不是逐行閱讀全部原始碼，也不是安全認證或完整動態效能測試。本輪沒有執行完整 Rust、mobile、cloud、edge suite，沒有重跑所有 native E2E、soak 或 release build。下列「結構已確認」不等於「已重現 production bug」；風險需要指定測試或量測才能升格為缺陷。

### 3.1 規模訊號

統計為自有、Git 追蹤的原始檔，排除 vendor、build、常見 Dart 產碼及 FRB 產碼，行數包含空行與註解。Desktop `lib` 為 960 檔、139,232 行，30 個 features；mobile `lib` 為 256 檔、34,825 行。數字用來定位範圍，不用來評分程式品質。

| Desktop feature | 檔案數 | 原始行數 |
| --- | ---: | ---: |
| workbench | 341 | 52,265 |
| settings | 81 | 16,100 |
| pull_requests | 79 | 12,514 |
| agent_status | 69 | 11,430 |

只看主檔長度容易漏掉實際耦合。按根 library 加上直接 `part` 檔案計算，不含 generated part：

| 同一 library 範圍 | 手寫 part 數 | 合計原始行數 |
| --- | ---: | ---: |
| WorkbenchController | 11 | 2,378 |
| TerminalRuntime | 21 | 4,207 |
| WorkspaceGitDiffPanel | 17 | 4,394 |
| mobile MobileRuntimeClient | 9 | 1,603 |

Desktop feature 層級的直接 package import/export 圖出現一個包含 23 個 features 的強連通群組。這包含 UI 整合、provider wiring 與 domain types，不表示 23 個 features 都有非法依賴，也不等同 Dart library 循環。此掃描未完整解析 relative imports、barrel exports 或 runtime provider graph；正確後續是區分 public domain/API 依賴與跨 feature 私有 implementation 依賴，而不是追求圖上零箭頭。[S1][S2]

## 4. 現況全景與應保留的設計

| 區域 | 現有責任與優點 | 後續重點 |
| --- | --- | --- |
| Desktop composition | `main.dart` 已集中部分 runtime/resource-manager/window-exit overrides；logging 在初始化前啟動 | 收斂 feature 對 app barrel 的反向依賴；明確 startup/stop owner |
| Workbench | 已拆 plan、queue、registry，並補 stale completion 與持久化順序測試 | 從共用巨型 state/mixin 拆成真正獨立的能力 owner |
| Terminal | session/title/output/buffer 已有局部介面、分流與可見性管理 | 分清 session I/O、renderer、shell preparation、資源回收 |
| Native/Rust | Cargo workspace 已分 native、core、CLI、mobile-native；sidecar 可獨立執行 | 不把所有核心操作繼續堆在 ServerActor 或通用 RuntimeStore |
| 資料儲存 | runtime.sqlite 是多個業務領域的權威；Drift 保留 local prefs/legacy migration | 為各類寫入明訂 revision、冪等、事件與恢復契約 |
| Mobile | per-host 連線 owner、foreground retry、epoch、cleanup timeout 與窄 client surfaces 已存在 | 統一 direct/relay/restart 的狀態轉移契約，不共用桌面 UI state |
| Cloud/edge | API、授權與 relay 分開；edge 有 frame/connection 上限；已有跨語言 relay fixture | 保留控制面與資料面區分，補端到端故障驗證與 CI 觸發策略 |
| Agent integration | Rust 已有 agent registry；Dart hook/status 已於 2026-09-20 收斂成 per-agent status + managed-hook adapters，並有 architecture guard | 下一步收斂 Rust/Dart capability schema 與跨層 fixture，減少 launch/quota/UI 等剩餘散落修改 |
| CI/release | 已有 analyze、產碼重現、測試分片、coverage report、舊 host compatibility、cloud PostgreSQL tests、簽章更新檢查 | 補 architecture gate、adapter conformance、穩定效能基準與平台矩陣 |
| Landing/infra | Landing 是獨立 Astro package；部署已有 plan validation 與 recovery 路徑 | 低優先維持 release metadata 一致性，不混進 runtime refactor |

特別不能退回的既有設計：runtime 持有 durable PTY 與業務資料、單一共享 terminal/runtime transport owner、close/terminate 與 detach/release 的語意區別、desktop/mobile 各自的 UI 狀態、dev/release profile 隔離、optional cloud、mobile capability/allowlist，以及舊版 host 的降級行為。[S3][S4][S5][S11][S12][S14]

## 5. 優先盤點結果

優先級是實作順序，不是漏洞嚴重度：P0 為下一輪前先補的驗證護欄；P1 為跨系統 correctness/可用性；P2 為可維護性與可量測效能；P3 為後續整併。

### A. P0：把架構規則變成真正的合併護欄

**結構已確認。** `runtime_architecture_guard.dart` 已檢查 runtime/platform、跨 feature Workbench infra、application/presentation 等方向。但本輪搜尋 `.github`、`tool`、`test` 與 makefile 的直接引用，沒有找到 CI 呼叫它；PR static job 現在執行 format、max-lines ratchet、analyze。現有 guard 多以字面 import 路徑判斷，application 規則並未完整追蹤轉出口；例如 app provider barrel 本身會 export presentation terminal providers。[S1][S15]

**方向。** 將 guard 接進 PR/merge 必經工作，為 guard 建立最小 fixture 測試，辨識 package/relative imports 與 barrel export；分層政策用明確目錄或 public API 定義，不靠不斷追加特定檔名字串。保留既有 max-lines baseline，但不能把「低於行數上限」當作架構改善。

**第一批測試。** fixture 分別放入直接違規、relative 違規、經 export 間接違規與合法 domain import，前三者必須失敗，最後者通過；加入 workflow contract test，避免檢查步驟日後消失。完成標準是 CI 真正執行、規則本身有回歸測試，而不只是本機曾跑綠。

### B. P1：先釐清錯誤與恢復契約，再統一重試行為

**行為已由原始碼確認，缺陷仍待情境測試。** Socket lifecycle adapter 將 connection-closed 與 request-timeout 轉成同一種 `RuntimeHostLifecycleTransportException`；service 對該型別視為 expected shutdown disconnect。另一方面 `runtimeSnapshotStream` 會捕捉所有讀取錯誤並重試，刻意不對訂閱者 emit error。[S6][S7]

兩種政策各有合理動機：快速關閉不要卡 UI，snapshot stream 不應因一次 IPC 失敗永久死亡。但 timeout 並不能單獨證明 shutdown 已被接受；decode/schema、權限或不支援能力的錯誤，也未必適合一直當作可恢復斷線。

**方向。** 先區分 accepted shutdown、confirmed stopped、connection lost、timeout/outcome unknown、unsupported、authorization、invalid payload。將 wire error translation 放在 adapter，application 只處理具體政策；UI 透過健康狀態看到正在重連、資料陳舊或需使用者處理。既有 transport exception 的合併不應抹除業務決策必要資訊。

**第一批測試。** 真正 socket adapter 到 service 的 conformance 測試：未送達的 timeout、送達後斷線、正常 accepted reply、busy/cancel、恢復後重新同步。對 unknown outcome 定義產品行為：可以關閉 UI 並保留 runtime，但不得把它誤稱為 force-stop 成功。保留「慢 cleanup 不阻塞視窗消失」的現有回歸。另對 snapshot permanent failure 測試有界重試與可觀測 health，不把全部錯誤直接拋給 UI。

本輪的 widget fake/production exception 不一致正好顯示：service fake 與 UI fake 各自綠燈，仍不足以證明 adapter 契約一致。

### C. P1：Rust ServerActor 的排程與資源所有權

**結構已確認，尚未量測這一版的延遲。** `server_runner.rs` 使用 unbounded command inbox，逐筆 `actor.handle(command).await`；ServerActor 同時持有 PTY、clients、workspace jobs、orchestration、quota、config transfer、mobile gateway、account/push、resource monitor 等狀態。[S8]

不能說 runtime「完全沒有 backpressure」：runtime mutation queue 已有 256 筆上限，會取消斷線 client 的 queued mutation，並把實際工作 spawn 到 worker；PTY output 也有 batch byte cap 及獨立 bounded delivery。缺口是其他流量如何進入主 inbox，以及各 handler 等待時控制面是否仍有明確服務保證。[S8][S9]

**方向。** 保留 actor 的 state ownership，先把 session/output、workspace lifecycle、orchestration execution、account/mobile 等能力的 job ownership 與 completion message 明確化。對重工作設定有界 admission、deadline、cancellation 及失效 completion 規則；必要時分離控制命令和高頻事件入口。不要直接把全部 mailbox 換成 bounded channel，卻讓 completion/quit 被堵住造成自鎖；也不要把每個 request 都無限制 spawn。

**第一批測試。** 以可控制 barrier 阻塞重工作，同時送 status、terminal input、quit/cancel，驗證控制路徑仍可前進；灌入超額工作驗證排隊有界、拒絕有 typed code、disconnected client 不再被執行；舊 generation 的 completion 不可覆蓋新 owner。量測並記錄排隊時間、執行時間、in-flight 數、pending bytes、p95/p99；門檻先建立可重複基準，再設定，不虛構目前效能數字。

### D. P1：資料權威、交易、冪等與事件應以業務操作定義

**現況。** Rust RuntimeStore 有 transaction-based domain operations，completion replay 也已有修正。`complete_orchestration_dispatch` 驗證 assignee/狀態，在同一交易更新 dispatch、task、dependents 與 queued messages。不能把這些工作當成未做。[S10]

**剩餘跨系統問題。** application 內的 serial queue 只約束該 owner 的呼叫順序，不是跨 desktop/mobile/CLI 的全域交易契約。Drift migration 用完成旗標與可重試流程；runtime view prefs 同時有 local 保存、runtime revision 與能力降級，這些政策需要可驗收的 authority 表，不宜各區塊各自猜測。[S4][S11]

**方向。** 依 Workspace、Tab/Layout、Project、Orchestration、Shared Preferences 建立 operation contract：權威資料在哪裡、instance/revision 如何檢查、可否重播、衝突如何回覆、DB commit 和 event 發送先後、失敗補償及重啟恢復方式。讓 core 提供業務交易入口，限制 adapter 任意拼多個低階 mutation；但不要只把 RuntimeStore 拆成數個同樣能任意 SQL 的檔案。

**第一批測試。** 模擬 DB 已 commit 但 reply 丟失，重送只能產生一次業務效果；以錯誤 assignee、舊 instance、不同 payload 重播時必須遵守明訂規則；在 migration 中途、tab/layout 更新邊界及事件發布前後注入失敗，再重啟與同步。先補目前沒有覆蓋的情境，不重做 `c1d88258` 已涵蓋的 completion replay。

既有 desktop/mobile view-prefs revision 政策不同，這是既有產品契約，不得在「統一」時暗中改成相同衝突處理。

### E. P1/P2：擴充真正的跨語言協定驗證，而非只維護數份字串

**結構已確認。** Desktop 的 platform protocol、mobile protocol/client 與 Rust wire dispatch 各自宣告與解析資料。Desktop RPC 仍有 `String type` 和 `Map<String,Object?>` 的通用入口；mobile 雖已有 MobileTerminalClient/MobileWorkspaceClient 等窄介面，背後 concrete client 仍負責多種 RPC、WebSocket、relay 與授權更新。[S5][S12]

已有舊版 host compatibility，也已有 Rust/mobile/edge 的 relay crypto/cross-language fixture；不是從零缺少契約測試。現有 host compatibility script 固定檢查一個舊 host baseline，跨語言 relay fixture 則由專用 integration script 啟動。[S13][S14]

**方向。** 先建立版本化的 golden wire fixtures 與 capability matrix，再選擇適合的 shared DTO/schema/codegen；不必立刻引入一套全新 RPC 框架。高價值契約先涵蓋 workspace lifecycle、tab/layout、錯誤、revision、shutdown、binary frame 與 output resync。跨 Rust/Dart/edge 共用的是 wire 契約，不是把 desktop domain/UI classes 整包搬給 mobile。

**第一批測試。** 同一份 fixture 在 Rust producer、desktop/mobile consumer 上 round-trip，包含未知欄位、缺少 optional 欄位、舊版 capability、invalid payload。對 old-host/new-client 與 new-host/old-client 明確選出支援版本矩陣。對依賴真實 workerd/Flutter 的長測試安排 path-triggered 或 scheduled 工作，不把每個小改動都拖進全套 soak。

### F. P2：Workbench 下一步要拆 owner，不是再增加一層轉呼叫

**結構已確認。** WorkbenchController 主檔已縮小，但 11 個 parts/mixins 仍屬同一 library。Internals 持有 providers、cleanup、subscription registries、不同 mutation queues 與 navigation history；WorkbenchState 同時包含 runtime records、tabs/layout、selection、shared prefs、search、bootstrap/error。[S1][S2]

**方向。** 建議逐步形成四個責任範圍：Catalog/read model 管理 projects/workspaces 與同步；Tab/Layout 管理 tabs、preview、active pane 與必要的跨欄位不變條件；Selection/Navigation 管理每個視窗的選取及歷史；Resource lifecycle 管理 subscriptions、editor sessions、terminal leases 與退場。原 controller 先保留 facade API，讓 UI 無需一次大搬家。

**第一批測試。** 先對「切換 workspace 同時收到 tab removal」建立行為測試，再把對應 owning state 與生命週期一起移出，而不只是抽一個接受十幾個 callback 的 function。完成標準是每個 owner 能獨立 fake/test，跨 owner 有明確訊息或方法，舊公用 Internals 實際變小，existing red-race regressions 不退化。

不要機械式把 WorkbenchState 每個欄位拆成 provider。Layout、active tab、selection 之間的同步不變條件應先定義，否則只會增加跨 provider race。UI rebuild 的收益需以實際訂閱與 profiler 量測，不以 provider 數量判斷。

### G. P2：Terminal 與 Git/editor 各自有更大的實際 library

TerminalRuntime 合計 4,207 行，TerminalSessionHandle 同時提供 session lifecycle、buffer usage、visibility lease、title、restart 與 `buildView`。應逐步分出可測的 session I/O/recovery owner、renderer adapter、shell launch/delivery 與 view resource budget；仍保留單一 session instance，UI 卸載不能等同殺掉 durable PTY。[S5]

WorkspaceGitDiffPanel 合計 4,394 行，widget state 持有 history future/result/error、commit compare cache、AI commit message generation、source-control navigation 等。已有 cancellation/generation 防護，下一步應把 history/compare loading、cache invalidation 及命令行為移到相應 application owner，讓 presentation 專心顯示與輸入。[S16]

**第一批測試。** Terminal 用兩個 view/tab 與同一 session 驗證 detach、reattach、output ordering、cancelled restore、background updates 和 buffer release。Git/editor 用兩個 repository scope 的 controllable futures 驗證舊結果不會出現在新 scope、cache 不跨 scope 污染、重複 refresh 被合併。保留整檔/差異範圍與 side-by-side/單欄的獨立設定語意。

檔案大不代表一律拆分：localization tables、generated bindings 或已合理共置的測試不應排在這些 ownership 問題前面。

### H. P2：Mobile、agent 與設定整合採能力矩陣

Mobile HostConnectionController 已有 retry timer、lifecycle epoch、opening attempt 及 dispose；應先補 direct/relay 切換、background/foreground、host restart 與 provider dispose 交錯的轉移矩陣，再評估縮小對 concrete MobileRuntimeClient 的依賴。不要把 desktop 的 keepAlive/window 模型直接套進 mobile。[S12]

Agent 支援仍分散在 Dart AgentType、安裝/launch、用量/UI 與 Rust registry，但 hook/status 這一段已在 2026-09-20 落地 adapter seam：每個 Agent 各自擁有 `normalizers/<agent>_agent_hook_normalizer.dart` 與 `managed_hooks/<agent>_managed_agent_hook.dart`，共享 normalizer/lifecycle/identity/installer/scripts 不再允許 `AgentType.<agent>` 分支，並由 `tool/quality/agent_extension_guard.dart` 強制檢查。Devin 的 status policy 與 Windows Git-Bash→cmd bridge 也都回到 Devin 自己的 adapter，不再混在中央邏輯。後續仍應以 Rust registry 及各能力的真實 owner 為基礎建立「agent × launch/hook/status/usage/restart」契約表，並用跨層 fixture 收斂 Rust/Dart 行為；不要把已拆出的 per-agent policy 再合回新的中央 switch。[S17]

Settings 同時是多功能的設定聚合與 UI 整合點。允許聚合各 feature 的公開設定型別，但避免業務層反向依賴設定頁面或 app barrel。local-only UI prefs、runtime operational settings、portable cloud configuration 應分清 serialization 與 authority，而非強行放進同一個全域 SettingsController 寫入佇列。[S4][S11][S18]

### I. P2/P3：Cloud/edge、release/portable 與可觀測性

Cloud API 已承擔 account/auth、runtime registration、subscription、configuration 與 relay grant 控制面；edge Durable Object 處理 relay connection/forwarding，已有 frame size、mobile connection cap、grant expiry 及舊 connection replacement。這些邊界應保留，不應為本輪桌面重構改成更多微服務，也不應讓 cloud 成為本地工作必需的同步入口。[S14][S19]

Release 已有 signed manifest/key 檢查、artifact hash 與平台 build/verification 腳本；portable、macOS/Windows/Linux、standalone runtime 與 mobile 的發布契約需要矩陣化與來源集中，而不是一支龐大的萬用 shell。Landing release links 和 package manager metadata 應由同一 release plan 導出。infra 已有 plan validator/rollback workflow，本輪僅盤點整合位置，沒有檢查 production secrets、雲端權限或實際部署狀態。[S20]

可觀測性應涵蓋跨層 operation/request ID、queue wait、執行時間、outcome、reconnect reason、resource owner 與 cleanup 完成狀態；不能為除錯直接記錄憑證或全部 prompt/terminal payload。既有 snapshot retry 可新增健康資訊而維持 stream 存活。效能 CI 目前明確以 main 趨勢採樣而非硬性阻擋，應先為可重複的 close responsiveness、queue admission、cancelled search 等建立決定性測試，再考慮穩定 runner 上的效能門檻。[S7][S21]

## 6. 建議目標架構

這是依現有模組演進的目標，不是要求全面搬資料夾或一次拆成多個 package。

```text
Desktop composition root                      Mobile composition root
  Feature UI + per-window state                 Per-host UI + app lifecycle
  Feature use cases / read models                Narrow runtime client surfaces
  Ports + lifecycle owners                      Ports + connection owners
           |                                              |
           +---------- typed, versioned contracts --------+
                                  |
                   Runtime adapters / transport
                                  |
          Rust admission + session/job supervision
                                  |
        Domain use cases + transaction/idempotency contracts
                                  |
          Runtime SQLite / PTY history / filesystem / Git

Desktop local adapters -> native Rust FFI -> local Git/files/search
Optional cloud: account/configuration/grants control plane
Optional edge: authorized encrypted relay data path
```

重要依賴原則：UI 可以依賴 application 公開 API；application 依賴自己的 domain/ports，而不是透過 app barrel 取得任意 presentation/runtime。Composition root 負責綁定 concrete adapter。Runtime 的 durable mutation 與多 client conflict policy 留在 runtime/core；每個 client 的選取與畫面偏好不因此全部集中到 server。Desktop 既有本地 Git、檔案與搜尋仍可經 native FFI adapter 執行，不能把圖中的 runtime 資料權威誤解為所有本地操作都必須繞一次 socket。

## 7. 分階段路線與完成標準

| 階段 | 交付 | 第一個可驗證情境 | 完成標準 |
| --- | --- | --- | --- |
| 0：護欄 | architecture CI wiring、guard fixtures、smoke DB fixture 隔離 | 間接違規 import 必須失敗；測試結束 DB 被正確關閉 | guard 為必經工作；67 個相關回歸保持綠；warning 原因明確 |
| 1：邊界契約 | lifecycle error taxonomy、adapter conformance、核心 wire fixtures | timeout 未送達不等於 accepted shutdown | 錯誤語意不丟失；快速視窗關閉不退化；舊版 host 有明確降級 |
| 2：runtime correctness | actor admission/job owner、transaction/replay 矩陣 | 慢 mutation 與 status/quit 同時發生；commit 後丟 reply | 控制面可前進；佇列與資源有界；重播不重複業務效果 |
| 3：Workbench 能力拆分 | Catalog、Tab/Layout、Selection、Resource lifecycle owner | workspace 切換交錯外部移除事件 | 狀態與 lifecycle 一起移出，controller 保留 facade，測試不依賴大型 internals |
| 4：UI/client 熱區 | Terminal session/render 分離、Git/history loader、mobile transition tests | 隱藏 terminal 持續工作；換 repo 不顯示舊 diff | 正確性、資源上限、rebuild/延遲基準皆有證據 |
| 5：擴充與發布 | agent capability matrix、release/platform contract、可觀測性 | 新 agent 或平台不必到處加判斷；不相容 artifact 被拒絕 | 已有行為不變、少量明確擴充點、CI/文件可追蹤 |

階段不是大批提交單位。每階段仍依 red test、最小實作、focused regression、相關 integration、format/guard/diff check 的順序小批提交。沒有重現的效能風險先做量測，不以猜測新增 queue；不需更動 native/protocol 的批次不要重跑或修改整套 binding。

## 8. 下一輪可以直接執行的第一批

建議從階段 0 開始，工作範圍限於 architecture guard 的 fixture tests 與 CI wiring。先確認當前 HEAD/dirty state，另開專用 worktree，保留目前未提交的 orchestration 測試。先寫 workflow/guard red tests，再接上檢查，最後跑 guard tests、既有 architecture guard 和 `git diff --check`；沒有改 generated provider/FFI API 就不產生無關產碼差異。

下一個實質 correctness 批次優先處理 lifecycle timeout/outcome contract，而不是再抽一個 sidebar helper。Rust actor/transaction 工作可在契約護欄完成後進行；不要將另一位正在做的 orchestration replay 工作混入同一批。

## 9. 不建議的路線

不建議推倒重寫、把本地功能改成依賴 cloud 的微服務、一次替換 Riverpod/Flutter/SQLx、為了行數把所有 methods 分成小檔、建立萬用 event bus/serial queue、機械式把每個 state 欄位拆 provider，或為了讓測試綠而刪掉現有 race/cleanup/visible-message assertions。

評估每批是否值得提交時，回答三件事：哪個 owner 變得更清楚？哪個跨層契約現在可測？哪個真實行為或失敗情境獲得保護？如果只增加檔案與 callback，卻沒有這三者之一，應停下來重新選邊界。

## 10. 原始碼依據

以下路徑相對於 worktree，行號對應程式碼基準；後續提交可能移動行號。統計來自本輪唯讀 inventory，詳細暫存位於 `.dart_tool/architecture-audit-20260911.json`，不作為 production 資產或測試 coverage 證明。

- [S1] `lib/src/features/workbench/application/workbench_controller.dart:101-140`；`lib/src/features/workbench/application/workbench_controller_internals.dart:7-137`；`lib/src/app/providers.dart:1-25`。
- [S2] `lib/src/features/workbench/application/workbench_state.dart:12-41,68-97`。跨 feature 靜態圖以自有 `lib/src/features` 的 package import/export 建立。
- [S3] `lib/main.dart:23-69`；`lib/src/app/app.dart:37-58`；`rust/Cargo.toml`；`mobile/pubspec.yaml`；`cloud/Cargo.toml`。
- [S4] `docs/architecture.md:28-46`：runtime authority、local Drift、view-prefs revision、activity merge、project/workspace lifecycle。
- [S5] `lib/src/features/workbench/presentation/terminal_runtime.dart:1-56,58-142`；`lib/src/platform/runtime_host/protocol/terminal_host_protocol.dart:200-221`。
- [S6] `lib/src/features/runtime_host/infra/socket_runtime_host_lifecycle_client.dart:20-57`；`lib/src/features/runtime_host/application/runtime_host_lifecycle_service.dart:163-188,284-315`。
- [S7] `lib/src/shared/infra/runtime/runtime_snapshot_stream.dart:33-44,63-95,99-124`。
- [S8] `rust/alera-cli/src/terminal_host/server_runner.rs:40-55,137-156`；`rust/alera-cli/src/terminal_host/server.rs:173-249`。
- [S9] `rust/alera-cli/src/terminal_host/server/runtime_mutation_queue.rs:12-28,40-77,93-173`；`docs/architecture.md:54-56`。
- [S10] `rust/alera-core/src/runtime/orchestration_dispatch_store.rs:431-494`；`rust/alera-core/src/runtime/store.rs:20-83`；既有提交 `c1d88258`。
- [S11] `lib/src/shared/infra/runtime/runtime_state_migration.dart:75-127`；`lib/src/features/workbench/infra/runtime_workbench_view_prefs_repository.dart:13-44,64-91`。
- [S12] `mobile/lib/src/features/runtime/application/host_connection_controller.dart:30-73,93-148`；`mobile/lib/src/features/runtime/infra/mobile_runtime_client.dart:36-64,108-142,154-170`。
- [S13] `tool/ci/host_compatibility.sh:4-8,38-47`；`.github/workflows/pr.yml:314`；`rust/alera-cli/src/terminal_host/relay_cross_language_tests.rs:31-35`。
- [S14] `edge/src/runtime_relay.ts:19-20,33-115,118-181`；`edge/package.json:6-12`；`mobile/test/relay_adversarial_end_to_end_test.dart`；`docs/architecture.md:32`。
- [S15] `tool/quality/runtime_architecture_guard.dart:139-188,191-231`；`.github/workflows/pr.yml:68-78,143-158,199-207,238-241,270-273`。CI direct-reference 搜尋未找到 `runtime_architecture_guard` 呼叫。
- [S16] `lib/src/features/workbench/presentation/workspace_git_diff_panel.dart:39-55,77-137`。
- [S17] `lib/src/features/agent_status/domain/agent_status.dart:16-27`；`lib/src/features/agent_status/infra/agent_hook_event_normalizer.dart:38-76,107-165`；`rust/alera-cli/src/terminal_host/orchestration/agent_registry.rs:1-126`。
- [S18] `lib/src/features/settings/application/settings_controller.dart:19-44,473`；`packages/alera_configuration`；`lib/src/features/configuration_sync/application/configuration_sync_controller.dart`。
- [S19] `cloud/src/api.rs:17-74,81-106`；`.github/workflows/cloud.yml:30-46,68-81,100-113`。
- [S20] `lib/src/features/updater/infra/desktop_update_service.dart:104-129,255-283`；`.github/workflows/release-cut.yml:431-447,861,1568-1585`；`.github/workflows/cloud-deploy.yml:409-440,530-540`；`landing/package.json:5-10`。
- [S21] `.github/workflows/startup-performance.yml:3-9,43-45`；`integration_test/windows_runtime_close_visibility_test.dart`；`integration_test/terminal_restore_benchmark.dart`；`integration_test/quick_open_benchmark.dart`。
