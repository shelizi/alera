**我建議保留現在的 Flutter／Riverpod＋Rust sidecar 架構，重點改成「拆清責任、狀態所有權和生命週期」，不要全面換框架，也不要只把大檔案切成更多小檔案。**

以本次讀取的 `main`（`8e4e89e8`）和 `refactor/runtime-boundary` 工作目錄來看，最值得優先處理的是：

**Runtime 共用邊界 → 關閉與終端生命週期 → Workbench 狀態拆分 → Rust 請求與儲存責任。**

目前 `refactor/runtime-boundary` 已經開始搬移 protocol，但仍有未提交修改；這批應先收尾，不宜另外啟動一份重疊的大重構。

## 一、最優先：完成 Runtime 邊界，不讓其他功能依賴 Workbench 內部

### 現在的問題

目前共用的：

`lib/src/shared/infra/runtime/runtime_host_providers.dart:1–11`

直接 import Workbench 裡的實作，而且對外回傳具體類別：

```dart
SocketTerminalHostClient runtimeHostClient(Ref ref)
```

這代表共用 Runtime 的依賴方向仍然是：

```text
帳號／設定／專案／Agent 等功能
                ↓
      共用 Runtime Provider
                ↓
     Workbench 裡的 Socket 實作
```

另外，`SocketTerminalHostClient` 不只是 Socket client。它同時處理 terminal/runtime 連線、pending requests、heartbeat、事件、capability 和退出期間的流量控制，主檔還帶著 12 個 `part`。  
依據：`terminal_host_client.dart:24–93`。

### 建議怎麼改

目前把 protocol 搬到：

```text
lib/src/platform/runtime_host/protocol/
```

**方向正確，但這只是第一步。** 後續應把「中立介面」和「具體傳輸實作」也分開，讓各功能只拿需要的能力。

建議形成這樣的依賴：

```text
Feature Controller／Service
            ↓
Feature Repository／Runtime Adapter
            ↓
中立 Runtime 介面
            ↓
共用連線、請求追蹤、重連與事件傳輸
            ↓
Rust Runtime Host
```

例如一般資料讀寫只依賴 Runtime RPC 介面；啟停服務使用既有的 `RuntimeHostLifecycleClient`；終端功能使用 terminal session 介面。不要讓每個功能都拿到整個 `SocketTerminalHostClient`。

**介面拆開，不代表各自新增 Socket。** 底下仍應共用既有的連線管理與控制／終端輸出通道，避免為了分層反而增加連線、heartbeat 和重連競爭。

這一階段我會限定為依賴重構：**不改 wire format、不改 protocol version、不順便改重連策略。**

### 現有架構檢查還需要補強

工作目錄中的 `tool/quality/runtime_architecture_guard.dart` 已經在檢查舊 protocol 路徑和新 protocol 的 feature import，但目前檢查範圍仍很窄：只針對特定字串及單一 protocol 檔案，還不是完整的依賴邊界檢查。  
依據：該檔案第 13–44 行。

後續應把規則擴成：中立 Runtime 層不能反向依賴 feature；新增檔案、相對路徑和 `export` 也要納入。否則這次搬完，之後仍可能從別的入口重新耦合。

---

## 二、第二優先：把「退出決策」與「執行關閉」分開

### 現在不是完全沒有設計，而是規則逐漸集中

`runtime_host_lifecycle_service.dart:147–209` 已經處理了不少重要情境：

持久化 host、保留 Runtime、狀態探測失敗、push-only、仍有 agent/session/job、取消、保留背景執行、強制停止。

而且近期「使用者確認退出後，先讓視窗消失，再等待較慢的清理」也已經落在程式中。**這些修正應保留，不應為了重構重新改寫行為。**

我建議把這段拆成兩部分：

**退出決策元件**只根據 Runtime 狀態、設定及使用者選擇，決定下一步。它不操作視窗、不送 RPC，也不直接建立對話框。

**退出執行元件**負責顯示確認、暫停一般請求、隱藏視窗、detach／shutdown、清理及錯誤收尾。

執行階段可以明確表示為：

```text
正常
  → 檢查狀態
  → 等待確認（需要時）
  → 已確認退出
  → 分離／停止 Runtime
  → 完成
```

取消則回到正常狀態，並恢復原本的服務。

最重要的不是 enum 名稱，而是建立可測試的約束：

**重複按退出只能共用同一個退出流程；取消不能留下半停止的 client；確認退出後不能又被背景刷新觸發重新啟動 Runtime。**

這種把複雜行為抽成獨立元件的方式符合 Flutter 的責任分離建議；但沒有必要替所有簡單操作再加一層 use case，官方也將 domain layer 列為依複雜度選用。

### 終端也要同步釐清四種不同操作

我建議介面明確區分：

| 操作 | 應有語意 |
|---|---|
| 隱藏／切換 tab | 不再繪製，不代表結束工作 |
| 釋放本地 view／buffer | 回收 UI 資源，不代表終止 PTY |
| Detach session | 此 client 離開，Runtime 工作可繼續 |
| Terminate session | 明確結束 shell 與其子程序 |

目前程式已經有部分區分，尤其 `TerminalRuntime` 的 release 語意，以及集中在 `closeWorkspaceTabs` 的清理入口；這些應成為新介面的基礎，而不是另開第二套清理路徑。  
依據：`terminal_runtime.dart:185–210`、`AGENTS.md:174`。

---

## 三、第三優先：Workbench 要拆物件，不只是繼續拆 `part`

這是本次檢查最明確的結構問題。

`workbench_controller.dart` 主檔只有 132 行，但第 40–68 行把 11 個手寫 `part`／mixin 組成同一個 Controller。實際上的訂閱、watcher、tab 對應、載入／關閉旗標和導航歷史，又集中在 `workbench_controller_internals.dart:40–62`。

**所以「主檔變短」不等於責任已經分離。** Dart 的主檔和它的 `part` 仍然是同一個 library，共用 private 可見範圍；`part` 本身不會建立獨立的模組邊界。

### 建議逐步抽出三個責任

**Workspace 訂閱與同步管理**  
負責 project/workspace/tab 訂閱、外部 worktree 變動和資料同步。清楚記錄每個訂閱的擁有者，workspace 消失時集中釋放。

**每個 Workspace 的 tab／layout 狀態**  
負責開關 tab、分割面板、焦點及 layout。不要為了某個 workspace 的 tab 變化，就必須經過所有專案共用的控制物件。

**選取與導航狀態**  
負責目前 project/workspace、前進後退和切換歷史，不直接負責建立 worktree、終止程序或存取所有 repository。

原本的 `WorkbenchController` 可以先保留為相容入口，把方法逐步委派出去；等呼叫端遷完再縮減。**不要一次修改所有畫面與 provider。**

同時要避免另一個極端：拆成三個 Controller，卻各自維護一份完整 `WorkbenchState`。我的建議是保留單一權威資料來源，各元件只擁有自己的狀態或衍生檢視。

### Riverpod 生命週期要配合真正的擁有者

參數化 provider 的快取會隨參數組合累積，Riverpod 官方也提醒應注意自動釋放或明確的生命週期管理。

對 Alera，我會採取：

**畫面資源跟畫面走；workspace 資源跟 workspace 走；PTY 跟 Runtime session 走。**

不能把所有 provider 一律改成 `autoDispose`，更不能因為 tab 沒有人觀看，就間接終止仍在執行的 agent。

---

## 四、值得先做的小批次：統一 Snapshot 刷新入口

這一項改動範圍比 Workbench 全面拆分小，而且有明確的測試切入點。

目前已經有 `RuntimeChangeCoalescer`，不是缺少 debounce、合併或重試機制。它已經處理批次執行、in-flight 和 dirty 後續刷新。  
依據：`runtime_change_coalescer.dart:30–55、105–129`。

但在 `runtime_snapshot_stream.dart`：

- 事件觸發的刷新走 `coalescer.schedule()`。
- 初次載入直接呼叫 `refresh()`。
- retry timer 也直接呼叫 `refresh()`。

依據：第 61–100 行。

**這是值得驗證的競態風險，不是本次已重現的錯誤。** 初次載入或 retry 與事件刷新是否可能重疊、較舊請求是否可能比較晚返回，應先用測試確認。

建議讓這三種入口共用同一套「同時只執行一次、期間變更再補刷」機制，並加入請求世代識別，避免 reconnect 或 scope 改變後的舊結果覆蓋新狀態。

驗收情境很具體：先發出慢請求，接著觸發重連與新請求，再故意讓舊請求最後完成，確認狀態不會倒退。

另外，既有程式刻意不讓暫時 IPC 錯誤終止 stream，這個方向要保留；但可以另外提供資料是否過期、最後成功更新時間等診斷資訊，而不是讓「仍可恢復」看起來和「一直正常」完全相同。

---

## 五、再往下：分離 Terminal session 與終端畫面

`terminal_runtime.dart` 目前同時引入 FFI、I/O、isolate、Flutter、Ghostty、PTY 和 xterm；`TerminalSessionHandle` 也同時提供 `ensureStarted()`、`restart()` 和 `Widget buildView()`。  
依據：第 1–55、129–141 行。

我會把它分成兩個主要責任，而不是一口氣拆十幾層：

**Session 層**負責 attach、restart、輸入輸出、執行狀態和錯誤恢復。

**View 層**負責 emulator、繪製、焦點、搜尋、選取、viewport 和本地 buffer 預算。

既有的 visibility lease、buffer budget、背景狀態更新和輸出批次機制應保留。目標是讓「關閉畫面」「回收 buffer」「終止程序」可以分別測試，不需要為了測 session 邏輯建立整個終端 Widget。

這項重構的直接收益主要是**可測試性和降低生命週期誤用**；單純搬檔案本身不能宣稱會降低 CPU 或記憶體。

---

## 六、Rust 端：拆請求責任與交易邊界，不急著改成更多服務

Rust 端也有值得整理的集中點：

`terminal_host/server.rs` 約 1,670 行，`server/requests.rs` 約 1,240 行，`alera-core/src/runtime/store.rs` 約 2,100 行。

但它們並非完全沒有拆分。`server.rs` 已有大量專用模組，`requests.rs:78–91` 也已經有 deferred request 路徑，因此不需要另外發明一套背景工作框架。

### 請求處理

建議讓中央入口逐步只保留解碼、身分／權限驗證、路由和回應，各領域的操作規則放到對應 handler／service。

耗時工作沿用既有 deferred 機制，統一處理併發上限、期限、取消和完成回報。**不要在拆分時，順便把原本序列化的 mutation 全改成平行執行。**

取消語意尤其要明確：Tokio 的 `spawn_blocking` 工作一旦開始，`abort()` 並不能停止它，shutdown timeout 也只是停止等待；因此「UI 不等了」不能直接當成「底層作業已取消」。

### 儲存層

可以把 `RuntimeStore` 中不同領域的查詢逐步抽成模組，但仍保留：

**同一個 Runtime 資料庫，以及跨 workspace/tab/layout 操作所需要的完整交易。**

目前 `store.rs:20–55` 的 connection pool 上限為 4，註解也說明普通 mutation 由 actor 序列化。沒有量測之前，我不建議把加大 pool 或增加 writer 當成重構的一部分。

拆儲存程式碼，不等於拆資料庫；拆 handler，也不等於拆成微服務。

---

## 七、Git／Diff 畫面放在後面，抽掉剩下的業務狀態

`workspace_git_diff_panel.dart:77–96` 還持有 history future/result/error、commit compare cache，以及 AI commit message 產生狀態；第 105–127 行又處理 scope 切換時的一整批取消與重設。

目前已經有 Source Control Controller，也已合入 Diff lazy loading，所以建議是**把剩下的 Git history/cache 和 AI commit 操作移出 Widget**，而不是重做整套 Source Control。

Widget 留下文字輸入、焦點、展開收合與畫面配置；查詢生命週期、scope/revision 快取及過期結果判定交給可獨立測試的 Controller。

真正要追求效能時，應依狀態變動範圍切成獨立 Widget、縮小 rebuild 範圍，並保留 lazy rendering，而不是只是增加 `part` 檔案。這也符合 Flutter 的效能建議。

## 我建議的實際提交順序

| 批次 | 範圍 | 驗收重點 |
|---|---|---|
| 1 | 收尾現有 protocol 搬移與 architecture guard | 舊引用清除、wire format 不變、相關測試通過 |
| 2 | Runtime 中立介面與 provider 注入 | Feature 不依賴 Socket 具體類別、不增加重複連線 |
| 3 | 退出決策／執行流程分離 | 保留取消、背景執行、強制退出及既有 native window 測試 |
| 4 | Snapshot 統一刷新入口 | 重連、重試、事件同時發生時不倒退、不重複刷新 |
| 5 | Workbench 訂閱與 Terminal 資源所有權 | workspace/tab 移除可完整釋放，切換畫面不誤殺 PTY |
| 6 | Rust handler/store 與 Git 畫面剩餘拆分 | 權限、交易、相容性和既有分頁行為不變 |

每批先補能固定現有行為的測試，再抽取責任；**機械搬移、行為修正、效能調整分開提交**。涉及效能時，至少記錄關閉確認後視窗消失延遲、背景終端輸出下的畫面幀時間、RPC 排隊時間，以及重複開關 workspace 後的 buffer／訂閱數，而不是用檔案行數判斷成功。

**我的首選是先完成目前的 protocol 重構，再處理 `runtimeHostClient` 暴露具體 Workbench 實作的問題。** 這是後續拆生命週期、Workbench 和終端資源時最有用的基礎；此時直接大拆 Rust actor 或全面換狀態管理，風險都比較高。

本次是原始碼與工作目錄檢視，沒有修改檔案，也沒有重新執行測試或效能量測；上述競態與效能項目是後續驗證目標，不是已確認的回歸。