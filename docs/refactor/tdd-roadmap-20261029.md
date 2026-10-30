# Alera TDD 架構重構總計畫（2026-10-29）

> **最新 T2-D12 三線已整合（2026-10-30；Capability OFF）**：Host `RuntimeStore` 對真正由原 Session 觀測到的 transferred PTY Exit，以 Tab／Workspace／Session／Receipt／PTY Instance 精確 CAS 持久化 `lastTerminalTabExit`，status 可報 `pty_exit_observed` 但 **`applied:false,recoveryRequired:true`**，不等於移轉已確認或可刪 Tab **`f259d98df`**；Journal `inspectRecoveryInventory()` 唯讀檢索全部 Lock／Pending、無效檔名／錯配 Hash／毀損資料與本地 Archive，**不解鎖** **`535db1096`**；Mobile 新增每個 Phone View 的 24-byte 隨機 `viewLeaseId`，具名 Attach／Detach／Restart、Host Restart token guard；搬移 Tab Close 需已附著 UI 原 Token／Receipt，直接送 `tab.remove`、**不再先殺原 PTY** **`62ce1d3b3`**。D13 仍需真正 OS Window/多裝置/Host restart durable Lease、Host Crash 成敗補償與 PTY generation renewal、exited moved Tab 合法回收、Journal stale Lock 安全解除及完整 Mobile／Win／Mac E2E。仍 `supported=false`，只有格式化與 Git whitespace checks，**沒有 Cargo/Flutter tests、Analyze、Cargo check、Build、Win/Mac/Phone E2E、Push/Release，也不碰 dirty main**。最新交接：`docs/refactor/feature-progress-20261029.md` D12。

> **歷史 T2-D11 三線已整合（2026-10-30；Capability OFF）**：Host 自然 Exit／通用 explicit Terminate 不再隱式刪除已轉移 Terminal 的持久化收據 **`673566e07`**；Desktop Journal per-Tab exclusive `.lock` 新增 nonce/PID/UTC/Tab Hash，正常 finally 確認 owner 身分才清鎖、Recovery Inspector 僅供唯讀診斷 **`95ab5e5b6`**；Workbench Close 在同步 Tab 快照時取得同一 Terminal Handle 的已附著 UI View Token，經窄 Service/Repository Port 把 `expectedViewLeaseId` 與原 Receipt、Workspace 送給 `tab.remove`，一次有限重試不得換 Token，缺證據 fail closed **`3adc4f465`**；若共享 Handle 有 0／多個 mounted Surface 則拒絕把 Token 誤當最後觀看者 **`2ea227339`**。**D12 仍缺真正 OS Window/Device + Host restart durable Lease、Crash/PTY generation 權威恢復／確定失敗、已死 transferred Tab 合法回收與 Journal stale-lock 安全人工解除、Mobile 全流程與完整測試。** `tab.transfer.capabilities.supported=false` 不變。僅格式化與 Git whitespace check，未執行 Tests/Analyzer/Cargo check/Build/Win-Mac E2E，未 Push/Release，原始 dirty `main` 不動。接手讀 `docs/refactor/feature-progress-20261029.md` 最上方 D11。

> **歷史 T2-D10 三線已整合（2026-10-30）**：Host Session 加 per-Client **多 ViewLeaseId 集合**與 legacy unscoped Attachment 標記，同 Socket 多 UI View 個別 attach/detach，Mobile 可選轉傳 View ID **`dae1db212`**；Host `tab.transfer` 要求每個原附著 Client 都已具名，`terminate` 與 `tab.remove` 用僅剩唯一 ViewLeaseId 而非 transient Client/socket 判斷，收據衝突與持久 owner 分歧回 `recoveryRequired` **`c22cf91f3`**；Desktop 透過狹義 `TerminalHostViewLeaseClient` 隨機固定 View UUID、共享 Host 逐視圖釋放，Journal 提供唯讀中斷鎖／原 Plan 稽核檢查 **`6c5571280`**。**尚缺 Workbench `tab.remove` 到同一 View Lease 的 UI 綁定、跨 Host Restart durable Lease、PTY crash definitive resolution／合法 Generation Renewal／dead Tab 清理、Journal stale-lock 安全人工修復、Mobile 全流程與實際測試**。Host `supported=false` 保留。只有 `rustfmt`／`dart format` 和 Git whitespace check，未跑 Flutter/Cargo Tests、Analyzer、Build、Win/Mac E2E，未 Push/Release，未動 dirty `main`。下一輪依 `docs/refactor/feature-progress-20261029.md` 最上方 D10 接手 D11。

> **歷史 T2-D9 三線已整合（2026-10-30）**：Rust Host 把序列化 `tab.remove` 綁定真正要求者 Client ID，核對 live PTY instance／當前收據／唯一 attached Client，且 Workspace/Project／Managed／Sleep 清理保留任何尚有轉移收據的 Tab（原 PTY 已死亦不刪），來源 **`c9468fd77`／`3753d60d7`** → 整合 **`3f1cf30d7`／`75161c9bf`**；Host Attach、spawn-on-create 及一般 Restart 不能在相同 Session ID 下偷換 transferred PTY，失敗不誤刪 moved Tab **`f10fb0f52`**；Desktop 同 App Support 的 per-Tab 原子 `.lock`、成功核實後先本機稽核再清 Pending Journal **`f536c68b6`**。**尚非真正跨視窗/跨 Host 持久 Lease，也不能復活 Host Crash 丟失的原 PTY；一般 transferred Restart 與 dead transferred Tab 清理會保守拒絕。** D9 當時建議 D10 View ID、stale lock 修復、Mobile 兼容；實際最新進度以 D10 為準。 `tab.transfer.capabilities.supported=false` 保持關閉。本批只格式化及 Git whitespace check，沒有新增／執行測試、Analyzer、Cargo/Flutter 編譯、Win/Mac E2E、Push、Release，不碰 dirty `main`。

> **歷史 T2-D8 三線已整合（2026-10-30）**：Rust Host Workspace／Project／Workspace Tabs／Sleep／Managed cleanup 確認是否有附著的已轉移活 PTY，拒絕隱式刪除 **`4bd4a8f96`**；Rust Store＋Host 的移轉 SQLite 收據綁定隨機 Process Epoch＋`Session.instance_id`，相同 Session ID 但新 PTY 不再假確認轉移 **`463725075`**；Dart 同 Tab 狀態確認排他直到雙 Layout readback＋Journal 完整清理 **`0ffb9b3a3`**。**仍欠跨程序持久 Last-view Lease／確定失敗與可稽核 Journal 清理／真實 Crash 補償與恢復；`tab.transfer.capabilities.supported=false` 不變。** 僅執行格式化及 Git whitespace check，未新增／執行測試、Analyze、Build、Win/Mac E2E、Push、Release，也未碰 dirty `main`。D8 歷史狀態的後續以最上方 D9 最新入口為準。

> **歷史 T2-D7 三線已整合（2026-10-30）**：Rust Host `tab.remove` queued/active Actor fence 阻擋新 Attach/Terminate、轉移收據已提交卻缺失／不符 PTY 時回報 `recoveryRequired:true` **`291a1c4c2`**；Desktop 不清除未知 Mutation Journal、顯示需復原提示 **`c32d3662f`**；Terminal／Tab 契約同步 **`375c89568`**。**僅部分封住單 Host Actor 競態，不是跨程序／重啟持久 Last-view Lease，也沒有完成 Host crash 後 PTY owner 復原／確定失敗與 Journal 安全解除。** `tab.transfer.capabilities.supported=false` 不變；本批只格式化、Git whitespace check，沒有新增／執行測試、Analyzer、編譯、實機、Push、Release，也未動原本 dirty 的 `main`。下一批從 `docs/refactor/feature-progress-20261029.md` 最上方 D7 入口與 `docs/terminal-tab-cross-workspace-plan-20261029.md` 的 D8 未完成項目續做。

> **歷史 T2-D6 四線已整合（2026-10-30）**：Host `tab.remove` 對已轉移 Terminal 強制目前所見收據 `8a9e78d83`，Store 來源 Pane Exact Split Tree Prune `74f8062a9`，Desktop App Support Journal `File.create(exclusive:true)` `dd71d3401`，Workbench Desktop Close 傳 Snapshot 所見 Workspace/Receipt `c23dd4189`。**仍無跨程序全域 PTY Lease／Host Crash partial commit 恢復與真實驗證；`tab.transfer.capabilities.supported=false` 不變，不能宣稱已支援跨專案拖曳。** 只跑格式化與 Git staged whitespace check；未跑 Flutter/Cargo tests、Analyzer、Build、Win/Mac 實機；未碰原始 dirty `main`、未 push／release。

> **最新：T2-D5+ 補充三線已整合（2026-10-30）**：在既有 D5 Store／Workbench／Agent Watcher（`5b4d15db5`／`a7a673591`／`9d420bf8d`）之外，新合入 Host moved PTY 多視圖 Close 拒絕及 `tab.remove` 收據守衛 **`32f365b79`**、App 重啟後原操作 ID／布局持久 Journal **`db7eca96b`**、Mobile 所見 Workspace/Tab/Receipt 的安全關閉 **`ea881429b`**。仍缺真正跨程序 lease、Host crash partial-commit 的明確復原狀態、Journal 安全清理及跨平台核心測試／編譯。**`tab.transfer.capabilities.supported=false`，尚不對使用者開啟跨專案 Terminal 拖放。** 本批只格式化與 Git whitespace check，不碰 dirty `main`、不 push／release。

> **最新：T2-D5 三線平行已整合（2026-10-30）**：Rust Store 限定轉移不能修改其他 Pane 的 Tab 順序和 Active Tab，來源未修剪 Pane 時亦禁止改動分割幾何 **`5b4d15db5`**；Dart Workbench 核對雙 Workspace 完整 Tab/Layout 並二次讀回避免舊快照反寫 **`a7a673591`**；來源 Tab watcher 退役不得當作 PTY Session Exit，保留 Codex 追蹤 **`9d420bf8d`**。**Host `tab.transfer.capabilities.supported=false` 不變，跨專案拖曳仍停用**；跨程序 Last-view Lease、Crash 中途轉移恢復、App 重啟 mutation journal、來源 Split Tree prune 比對、Mobile 相容、實際 Flutter/Cargo 編譯與 Win/Mac E2E 仍待做。來源獨立分支及限制見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。本輪僅格式化＋Git whitespace check，未做測試、分析、編譯或發布。

> 狀態：執行中。工作分支 `refactor/tdd-boundaries-20261029`，基準 `3501b05550f6ca476528ca63c3bb7673c444b985`。
> 每次接手先讀此檔、`docs/refactor/tdd-progress-20261029.md` 與 `skills/alera-di-architecture/SKILL.md`，以進度文件中的下一個未完成驗收項目繼續。
> 2026-10-29 第二批已將 Q0/G1/R1 各自獨立工作樹的 verified commits cherry-pick 至 `refactor/tdd-integrate-20261029`。第一批表格僅作歷史狀態，**現況請先讀進度檔最上方〈第二批三線平行〉**。
> 2026-10-29 **第三批**同時切出 A0/Q0/G1/R1 四個獨立工作樹，A0 已達 5 touchpoints、Q0 max-lines 132、G1 已獨立狹義 SourceControl 讀取；R1 增進 shutdown/restart 控制面，主 inbox 仍 unbounded。**現況優先讀進度檔〈第三批四線平行重構〉**，本節既存 Spec 不變。
> **第四批 2026-10-29**：W1 同 workspace hydrate race、G1 六方法 index mutation、Q0 Git Diff UI History lifecycle、R1 bounded client line 已平行切分。**現況與驗證狀態先讀進度檔最上方〈第四批四線隔離 TDD〉**。W1/G1 已各 12／15 focused green；其餘測試未 complete 不標 PASS。
> 第四批 W1/G1 已整合為 `c845fbe2e`／`6e4f0d48e`。Q0 實際 oversized 165→164、offenders 132→131，無 baseline 重設。Rust 本機 request line 16 MiB 限制已有 Cargo 6 tests green；主 Actor inbox 仍 unbounded。
> **第四批已四線整合**：W1 `c845fbe2e`、G1 `6e4f0d48e`、Q0 `cb7ad9d97`、Rust R1 `89ac5306e`。Q0 60 Widget tests green、全域 164 oversized / 131 offenders，Rust 8 Cargo tests green；整合 Flutter 29 tests green。下批請以進度文件最上方第四批完成記錄為最新起點。
> **第五批進行中**：從上一批乾淨 HEAD `56f0658f9` 四線獨立 worktree 平行進行 W1 stale history cursor、G1 read-only diff page port、Q0 Git History actions 分拆、R1 localhost 多 client pending count admission。請先讀進度檔最上方「第五批四線隔離 TDD」以取得實際驗證狀態，不要把未完成的 Rust/Flutter 全套驗收標成 green。
> **第五批四線已整合（2026-10-29）**：W1 `35c8d1a03`、Q0 `0299ba00e`、G1 `c35ce883f` + preview follow-up `721b82b9a`、R1 `e369c2232` + test isolation `00cc090c9`；Mobile Hello shared fixture 更新 `262a8d16d`。整合 Flutter 23 green、Flutter shared wire fixture 7 green、Git History UI 60 green、Diff Surface 54 green+2 skipped、Rust Client+priority+deferred 20 green、Rust wire fixture corrected後48 green。整合官方 Q0 **162 oversized / 129 offenders**，baseline 未重設。最新事實與下一批分工請讀進度檔最上方。
> **第六批四線進行中（2026-10-29）**：從第五批整合 HEAD `7063e72df` 平行進行 W1 project activation invalidation、G1 two-method diff blob query、Q0 Explorer watcher ownership、Rust R1 localhost 32MiB admitted pending byte budget。Red→Green 與真實剩餘 blocker 一律以進度檔最上方第六批為準；原本 dirty `main` 不動。
> **第六批三條 Dart 線已整合**：W1 `5794f93b1`、Q0 `5a67f87c9`、G1 `6aea669b4`；整合 Flutter focused 26 tests green，兩個架構 guards green、官方 Q0 **161 oversized／128 offenders**，不改 baseline。Rust 新 byte-budget Red→Green 尚在 Cargo 多組回歸，勿誤認整批已完成。
> **第六批四線均已整合（2026-10-29）**：W1 `5794f93b1`（完整 Workbench Controller 133 pass）、Q0 `5a67f87c9`（完整 Explorer 78 pass）、G1 `6aea669b4`（Diff Surface 54 pass + 2 skipped）、Rust R1 `996962ef4`（Client 11 + Wire 48 + priority 2 + deferred 9 = 70 Cargo tests pass）；整合 Flutter 26 pass、兩守門 exit0；官方 Q0 **161 oversized／128 offenders**，baseline 未修改。此訊息覆蓋上一行「Rust 待驗證」的歷史狀態，往後從進度檔最上方第六批接續。
> **第七批四線進行中（2026-10-29）**：從上一批乾淨整合 HEAD `e38e8191c` 分別處理 W1 同工作區 Git Focus Probe race、G1 巢狀 Repo file revision narrow port、Q0 Branch Status Bar hierarchy part 拆分、R1 localhost socket connection admission cap。各線已新增真實 Red→Green 測試；具體驗證狀態請讀進度檔最上方「第七批四線隔離 TDD」，未完成完整回歸前勿誤記整合綠燈。
> **第七批四線均已完成並整合（2026-10-29）**：R1 `da33328ea`（socket 同時 32、Cargo 71 tests pass）；W1 `262b18526`（Git Focus Probe race，Controller + service 140 pass）；G1 `116dce5cd`（1-method file revision port, Diff Surface 54 pass + 2 skipped）；Q0 `29b455da2`（Branch status 538→456，Shell/Favorites 109 pass），全部 Red→Green，Dart 三線 analyze No issues；整合 Flutter 142 pass、兩 guards exit0。官方 Q0 **160 oversized／127 offenders**（前 161／128），baseline 未改。以進度檔最上方第七批為最新事實，下一批繼續四線獨立 worktree，不碰 `main` 的既存 dirty 資料。
> **第八批功能優先四線已整合（2026-10-29）**：依使用者指示跳過本批測試／靜態分析／效能量測及純拆檔，從 `54d6280c1` 平行實作 R1 未 admission localhost 64 MiB quota `48a45b5f3`、C1 Mobile 偏好值 timeout 讀回確認 `4437ee71e`、T1 共用 PTY 最後本機視圖才 kill `45941055a`、W1 過期背景 Layout 忽略 `a7ee48fbd`。**此批未驗證 green、不可直接發布**，詳細資訊請讀 `docs/refactor/feature-progress-20261029.md`；前面批次的 TDD DoD 是歷史驗證契約，不應誤認為這批有跑。
> **後續功能新增 T2（2026-10-29，僅列規劃）**：跨專案／跨 Workspace 拖曳既有 Terminal Tab，**PTY、Session ID、執行程序、scrollback、原 CWD 維持不變**，支援原子歸屬轉移、目的地選擇與 Undo。已將既有設計稿 `ec150a493` 的**單一文件**收編為 `docs/terminal-tab-cross-workspace-plan-20261029.md`；未合併設計分支的程式碼、未實作／測試本功能。Flutter drag/drop 可與 Rust Host transfer 契約設計平行，Store/PTY ownership、Workbench hydration/close 接線不得同檔平行修改。
> **T2 最新三線平行基礎已整合（2026-10-29）**：從 `061216941` 分支隔離實作 Flutter Sidebar/Project DropTarget（**`8c119f0a1`**）、Workbench Coordinator／Runtime narrow Port（**`97e02eedb`**）與 Rust Store 原子 Tab + 雙 Layout CAS（**`0fc27062b`**）。**此階段只具備基礎，使用者跨專案拖放仍被 feature gate 拒絕；`tab.transfer` Host Actor route、活 PTY Session owner／agent hook／restart／pulse／雙側 watcher reconcile 仍未實作。** 先完成 T2-B2（唯一 Runtime/Store owner）才可接 UI、Undo。依使用者指示本批未跑測試、Analyze、編譯或實機；詳細交接見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。
> **T2-B2 最新三線已整合（2026-10-29；上行為歷史狀態）**：Host 活 PTY 轉移 RPC **`f3977fa99`**、Workbench 雙側 watcher fence＋能力探測＋連接 Controller **`8443c2f0e`**、Terminal 原執行路徑提示及 remint hook env **`683b99dc8`**。**Host 明確回報 `tab.transfer.capabilities.supported=false`，所以真正跨專案拖曳入口仍未開啟**。需先解決跨程序/視窗關閉競態、失聯重播、Agent Presence 更新、Undo、原路徑消失時重啟，並驗證 Flutter/Rust 編譯與實機。依使用者指示本批仍只做格式化與 Git whitespace check；詳細見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。
> **T2-D2 三線最新整合（2026-10-30；D1/B2 為歷史紀錄）**：Rust Host 已移動 PTY `terminate`／`createOrAttach`／`restart` 及 scoped `tab.remove` owner guard **`df69aece0`**；Dart Client 提供 PTY terminate＋Workbench Tab Close 的原視窗 Workspace 身分、可重用 Handle 同步更新顯示 Workspace **`817fcf7e1`**；其他視窗的 Watcher 發現轉移收據時讀取 Host 權威 Pane Layout，不反寫舊版本 **`333afb30e`**。Host `tab.transfer.capabilities.supported=false` 不變，**跨專案拖放仍不可用**。剩餘跨程序 PTY lease／Close/Detach、Host Layout 全域並發保護、Crash/Restart、Output、Undo、Contracts／編譯／回歸與實機；這批只格式化與 Git diff check。詳 `docs/terminal-tab-cross-workspace-plan-20261029.md`。
> **T2-D1 最新三線已整合（2026-10-29；B2 為歷史紀錄）**：Rust Host 避免 Workspace removal/shutdown race、保護轉移收據、提供同 Mutation＋活 PTY 證明的 `tab.transfer.status` **`43591a4be`**；Dart Timeout/Disconnect 權威查詢／Unknown Outcome 不中途重送 **`5a0f01bdd`**；Controller／Sidebar Undo 依最新收據反向移轉並盡量恢復原 Pane Group **`efce4bea6`**。**Host capability 仍為 `supported:false`，跨專案 Terminal 拖放尚不可用**。剩餘多視窗／多程序 explicit Close/Detach、Watcher/Output、Restart 失效路徑、契約／編譯／回歸與實機；依使用者之前決策不執行這批測試、分析或發布。詳 `docs/terminal-tab-cross-workspace-plan-20261029.md`。
> **T2-D3 三線已合入（2026-10-30）**：Rust Store `layout.upsert` 的已轉移 Terminal membership 交易守衛 **`4c6fc975d`**、Dart 原 Mutation 收據在未知結果期間阻擋新 Transfer **`76e093931`**、Dart/Host 原執行目錄不存在時不誤 Restart/Remint 並保留轉移後的 Tab **`0809bbe3f`**。**`tab.transfer.capabilities.supported=false` 仍未開啟**；後續 D4 仍需全域多程序 PTY owner/Lease、Crash recovery 一致性、UI/操作契約與實際編譯回歸。這一批只格式化與 Git whitespace check，未做 Flutter/Cargo Tests、Analyze、Build 或實機；以 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新 D3 為準。
> **T2-D4 三線已合入（2026-10-30）**：Rust Host `expectedTransferMutationId` 拒絕已移轉 Tab 的 A→B→A 舊收據終止 **`e74cda072`**；Dart Coordinator unknown outcome 需原收據 + 雙 Workspace 權威 Layout readback 才解除阻擋，新增 Controller 檢查狀態 **`ad35694ac`**；PTY client 傳送本視圖所見收據，Sidebar 加入狀態未知的「Check status」而非假成功 **`469b64c54`**。仍是 **Capability OFF、未編譯／測試／發布**；真正跨程序 Last-view Lease、Crash 後部分提交恢復、跨 App 重啟的未知 Mutation、Mobile 相容與 Win/Mac 驗證尚未完成。請讀 `docs/terminal-tab-cross-workspace-plan-20261029.md` 最新 D4，舊 D3 為歷史紀錄。
> 目標是可逐批合併的 TDD 重構；不改產品語意、不清理第三方既有工作樹、不自動推送或發布。

## Spec

- 維持 Flutter Desktop / Mobile、Rust runtime-host、Git Diff/Graph、Explorer、持久化 PTY 等既有功能及 UI。
- 降低跨層依賴、Fake 的必需模擬方法數、未受保護的重度工作、共享狀態與資源清理競態。
- 每個批次先建立最小可重現的紅測（功能、架構 guard、競態或契約），再實作、重跑綠測與回歸，留下可重跑指令。
- 有明確而可量測的 CPU/IO 熱點時優先 Rust；Flutter widget、Focus/scroll/listenable、Riverpod composition 及僅一兩次資料映射不強行跨 FFI。
- 多個 workstream 可平行 *調查／紅測／執行互不衝突的測試*。同一檔案、共享 Cargo target、Flutter generated output、Git index 禁止平行寫入。

## Design rules / Rust decision record

1. 不推倒重寫：保留四個 Workbench owner、單一 runtime PTY/session owner、SQLite shared pool 與 transaction 邊界、per-agent hook adapter、GitRustBackend concrete，以及舊版 host 相容。
2. Flutter application 只依賴窄能力 port。資料轉換可純 Dart；避免 widget 持有非同步生命週期、cache 與取消狀態。
3. Native Rust 合適位置：Git status/diff/history/diff algorithm、檔案索引/快速搜尋、批次 diff projection 與 native workspace I/O；仍需 benchmark + parity + cancellation + UTF-8/CRLF 測試後才能搬移。禁止因重構複製整份字串、建立同步 FFI 阻塞 UI、或更改既有 git credential helper 介面。
4. Rust server 有 `DeferredAdmission`、mutation queue、PTY 專屬背壓，不能把「主 inbox 無界」誤寫成所有路徑均無界；先量控制路徑 p95/p99、queue/in-flight/pending bytes，再設入站 admission。completion/quit/cancel 路徑不可自鎖。
5. 資料權威：project/workspace/tab/layout/orchestration/prefs 優先 runtime。明確 revision、instance ID、重播、commit/event、未知操作結果；desktop/mobile UI 狀態不強行共用。
6. 每個批次的 Git 修改只落在獨立 worktree 分支；驗收後按檔案清單 commit，是否合併 main 另行判斷，保留原本 main 的未提交變動。

## Workstreams（可平行的範圍、TDD 驗收）

| ID | Priority | 專責／相依 | TDD 紅測 → 綠測標準 | Rust 決策 | 狀態 |
| --- | --- | --- | --- | --- | --- |
| G0 | P0 | Architecture | `runtime_architecture_guard.dart` 原 2 項違規 → 0；Resource Tree label 更新、listener 清理/關閉不退化 | UI/provider port 保留 Dart | verified (batch 0) |
| A0 | P0 | Agents／Quality，與 G0 無檔案衝突 | synthetic fixture 正/反例通過；per-agent adapter 規則維持；新 touchpoint 需有減耦理由 | Agent launch 能力後續再研究，hook adapters 保留 Dart/Rust owner | guard verified；8 → 5 ongoing |
| Q0 | P0 | CI／品質 | max-lines 依 generated/locale/test/產品邏輯分類；新增檔案不灌水；基準不直接整批重設；逐步縮短已長檔 | 純品質工具不用 Rust | inventoried；134 violations remain |
| G1 | P1 | Git Capability Ports；先於 Git UI 大重構 | loader/cache/retention 只實作所需 narrow fake，成功同時不破壞單一 production backend；Git/history/scope 切換測試 | 重運算維持 RustGitBackend | history/compare ports verified；more ports pending |
| R1 | P1 | Runtime scheduler（Rust，與 G0/A0 可平行研究） | blocked heavy request + status/quit/cancel、超限拒絕、client disconnect、stale completion 確保成功；採樣 queue wait/latency | 全部 Rust | read-only audit complete；Rust TDD pending |
| W1 | P2 | Workbench | 針對 workspace switch/tab removed/late hydration 設 red race；縮 Owner Host 介面後保持狀態不變式 | 純 UI/owner Dart，IO 走 Rust | pending |
| T1 | P2 | Terminal | detach≠kill、兩個 view 的同一 session、背景 output/order、buffer release、restore 不回退 | PTY Rust owner；Renderer/畫面 Dart，必要 native parser 用 benchmark 再決定 | pending |
| T2 | P2 | Terminal 跨專案移轉；依賴 T1/W1/C1 Runtime/Store 權威一致性 | 拖放到其他 Project／Workspace 保留同一 PTY/Session、CWD/scrollback；原子更新 tab 與雙側 Layout、Session workspace owner，失敗不遺失 Tab，復原可逆；原有重排／Detach 不退化 | Rust Host + Store `tab.transfer`；Flutter Sidebar/Tab 目的地選擇與協調；不重建 Shell | **D1–D12 原始碼安全補強已整合，Capability OFF**；D12 增加原 PTY Exit 的 CAS 持久化證據、Journal 全目錄唯讀復原盤點、Mobile 具名 View Attach/Detach/Restart 和原 Lease/Receipt 直接關閉，避免 terminate-first；D13 仍欠真正 OS Window／多裝置／Host 重啟 durable Last-view Lease、Crash definitive resolution／PTY generation renewal、合法 exited Tab 退休、Journal stale Lock 安全解除及完整 Flutter/Cargo／Phone/Win/Mac E2E。詳 `docs/terminal-tab-cross-workspace-plan-20261029.md` |
| D1 | P2 | Git Diff / Editor | 長行 Myers、side-by-side/editable、scope cache/in-flight、第一差異與水平位置回歸；CPU projections 可基準後 native 化 | Rust 優先算法，UI Dart | pending |
| F1 | P2 | Explorer / Files | 隔離 nested worktree、模糊查找、reveal/persist、conflict/rename、watch cancellation、path traversal parity | index/IO 優先 Rust；UI Dart | pending |
| C1 | P2 | Contract / Store / Mobile | lost reply after DB commit / replay，direct↔relay、restart/resume、old/new host wire fixtures | Rust store/transport，Mobile view Dart | pending |

## 執行批次與依賴

- Batch 0（本次）：G0 與 A0 先各自寫紅測，再綠測；Q0 分類並落檔；G1 可先做邊界試點。並行的是互不衝突的調查/讀取/測試，不同檔案寫入先約定 ownership。
- Batch 1：G1 query port 穩定後拓展 mutation ports；R1 單獨 Rust 測試/metrics（不改 wire schema）。
- Batch 2：W1、T1、D1、F1 可在分支或獨立 worktree 並行，分別擁有自己的 source/test；同一 Git Diff/Terminal 測試需隔離資源。
- Batch 3：C1 共享 fixtures、cross-version roundtrip、baseline 性能；再選擇整合時機。
- 跨語言 API 異動須執行 `make frb-generate` 並提交 Dart bindings；本次 G0/A0/G1 不改 FRB，無需重新產生 native binding。

## 每批 DoD（Definition of Done）

- [ ] 紅測先失敗且失敗理由符合目標（保留 red 結果）
- [ ] 做最小實作；相同測試變綠
- [ ] focused Flutter/Rust 測試與架構 guard
- [ ] 必要時 build_runner 單次產碼、normalize_generated_eof、dart format
- [ ] `git diff --check`、受影響 package analyze、檢查 task/git status
- [ ] 更新 progress 的測試、風險、未完成、下一步與 commit SHA
- [ ] 僅提交 scoped file；不以成功的單元測試宣稱完整 Release／全平台合格

## 假設、限制與不做事項

- 初始主分支 2026-10-29 12:43 的 `3501b0555`，main 存在舊歷史文件未提交與其他測試檔，不觸碰。
- 舊 `docs/architecture-review-2026-09-11.md` 不是這版的即時測試結果；本文件記錄新的校驗。
- 不重寫 terminal emulator、Git backend、cloud 架構，不做沒有 baseline 的盲目 Rust 遷移，也不批次刪除既存資料／Worktree。
- 工作可能跨多個對話；務必逐批更新進度。每次接手以 git status 與實際測試重查，避免將曾通過視為永久通過。
