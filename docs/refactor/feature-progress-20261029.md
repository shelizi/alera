# Alera 功能優先續做：第八批四線隔離（2026-10-29）

> 使用者決策：暫停新增測試、靜態分析、效能量測與純粹為降低 500 行違規的拆檔；這一批改為四條互不衝突的功能實作。**本批尚未執行 Flutter/Cargo 測試、Analyzer、Guard、跨平台編譯或發行驗證，不得宣稱本批編譯／測試通過。**程式碼已格式化、做過變更檢視及 Git staged diff whitespace check。
>
> 起點：第七批已驗證、乾淨的整合 HEAD `54d6280c1815f5311b2907e86deded483648bd01`。實作分支均以此點切出。第七批交接時記錄的 `main` HEAD 為 `3501b0555`，但**第八批收尾實際查到 `main` 已由其他並行作業更新至 `3d232acff9822154c68049ee63c57ed5966b84e0`**，仍有既存未提交資料（包括 `third_party/xterm`）；本批**沒有**在 `main` 上提交、清理、merge、push 或 release。切勿以第七批的 `main` SHA 當作新現況。

## 第八批已落地功能與隔離提交

| 線路 | 實際使用者功能 | 位置與提交 | 嚴格範圍／尚待 |
| --- | --- | --- | --- |
| R1 Rust Runtime | localhost 連線除了原有同時 32 Socket、64 個已 admission commands、32 MiB 已 admission requests，另外新增**最多 64 MiB 未 admission/部分讀取中的請求內容額度**。讀取前按 4 KiB 區塊向共用 semaphore 嘗試拿配額；配額不足時直接拒絕該連線，而非一面持有其他部分 request、一面等待而互鎖。已完成的請求成功進入 Actor 才釋放未 admission 配額；取消、socket 關閉由 RAII 自動釋放。 | `rust/alera-cli/src/terminal_host/client_line_reader.rs`、`client.rs`；隔離 `e82f9c579` → 整合 **`48a45b5f3`** | 限制的是**按字節向上取整計費的 localhost pending payload**，不是 Rust 配置器的真實 heap bytes，也不含 BufReader、String/Vec 額外容量、其他 actor/輸出佇列；不包含 Mobile、Relay、Hook。配額壅塞下將關閉新讀取的 socket，呼叫端需重新連接。**本批未驗證 Cargo 編譯、控制指令行為**。 |
| C1 Mobile 跨端一致性 | Mobile 更新共享 Workbench 偏好值時若收到 request timeout，不直接重送可能已 commit 的 mutation，而是重新讀取 Runtime 的權威 view prefs 及 revision；只有提交的每個欄位都已反映才當作成功，集合欄位忽略排列順序；不相符或讀取失敗仍保留原本 TimeoutException。 | `mobile/lib/src/features/runtime/infra/mobile_runtime_workspace_sidebar_client.dart`；隔離 `f8205773f` → 整合 **`4437ee71e`** | 只涵蓋 **`workbenchViewPrefs.update`** 且仍能查詢的 timeout，不自動重送、不對所有 Create/Delete/Terminal write 推廣冪等；真正斷線或 server 舊版沒有讀回能力時仍須重連並重新整理。**尚未 Mobile 測試／編譯。** |
| T1 Terminal 生命週期 | 當本機有兩個 Tab／檢視共用相同 durable PTY session ID，明確關閉其中一個只釋放這個視圖，不終止其他仍存活的本機檢視；關閉 Workspace 也改經過相同判斷，最後一個本機檢視才保留原有的 terminate 行為。 | `lib/src/features/workbench/presentation/terminal_runtime_session_owner.dart`；隔離 `f3a927a31` → 整合 **`45941055a`** | 只根據**同一 Desktop runtime registry 的已建立 handle**判斷，跨程序/遠端 Mobile 看不見，無法保證全域多客戶端 reference count。顯式刪除 Tab 的外層 overlays/observer 清理仍為既有行為；**尚未 UI/PTY 整合驗證**。 |
| W1 Workbench | 背景 `loadLayoutForWorkspace` 等待 listTabs 與 layout resolver 時，如果較新的工作區選擇、分割或焦點變更已經建立 layout，舊結果直接捨棄，避免覆蓋新的版面配置；workspace 已刪除／controller 已 dispose 仍不套用。 | `lib/src/features/workbench/application/workbench_tab_layout_owner.dart`；隔離 `868cdd733` → 整合 **`a7ee48fbd`** | 精準處理「原本沒有 layout、背景補載」與較新 layout 的競態。尚不等於所有 tab/layout watcher 都有 transaction generation，也未驗證 UI／跨端回放行為。 |

## 本批整合狀態與後續

- 四條線的 commit 已 cherry-pick 至 `refactor/tdd-integrate-20261029`，分別為 R1 `48a45b5f3`、C1 `4437ee71e`、T1 `45941055a`、W1 `a7ee48fbd`。以上是**已完成的來源碼修改／Git 提交**，**不是**「已測試、可發布」的聲明。
- 第八批交接 HEAD `abc7c3f1f017c70a2bac340d614f1c176469f831`；整合樹與四條隔離工作樹 `git status --porcelain` 均乾淨。若其他代理還在更新 `main`，後續要合併必須先做新差異盤點，不能假設仍是 `3501b0555`。
- 這批無意擴大 Rust 化：手機 UI／Workbench 焦點狀態保持 Dart，只有 localhost protocol memory admission 由 Rust 控制。無 FRB API/schema 變動，未重新產碼。
- 下一輪功能優先：Mobile/Relay/Hook 的跨 transport quota 和 request lifecycle，跨端操作冪等／lost reply 結果確認，跨 process terminal owner 引用計數，Workspace Tabs watcher 的更完整 generation／版本化，Explorer worktree 隔離和 Git Diff 取消一致性。
- 如要合併 `main`、對外推送或發布，應另行處理本批未驗證的編譯與回歸風險；不可把第七批的 142 Flutter／71 Rust green 視為本批驗證。
