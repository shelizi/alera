# Alera T2 跨 Workspace Terminal Tab：一次收尾成果與最後驗收門檻（2026-10-30）

> **最新功能狀態（2026-10-31）**：T2 同一 Host 的桌面轉移功能已採用 `tab.transfer.capabilities` 協定協商；含完整 T2 實作的 Host 無 Windows／macOS／Linux 作業系統白名單，舊／不完整／離線 Host 仍被 Flutter 拒絕。以下 Gate A／B／C 與 `supported=false` 記錄屬**先前開發階段的歷史驗收資料**，不是當前程式的能力回覆。原 Host 身分不明、Crash、收據損毀或 View 不一致時，實際操作仍 fail-closed；macOS／Linux 原生 E2E 尚未據此宣稱通過。

## 2026-10-31 Gate A／B／C 接續實作與驗收（本文件最新入口）

**整合起點已核對**：唯一整合分支 `refactor/tdd-integrate-20261029`，原始 HEAD `806f5d7af16112839eda1badb7ed99dec873e463`，工作樹初始 clean。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0`，原本修改與未追蹤內容不動。**固定只使用 Gate A／B／C，不新增其他階段；`tab.transfer.capabilities.supported=false`、`livePtyTransfer=false`，絕不 merge／push／release。**

- **Gate A／Host 原 PTY 存在但持久收據遺失時的明確拒絕**：`terminal_tab_transfer_status` 對找不到 SQLite Tab、Tab 類型衝突或轉移 Receipt 被抹除均回傳 `applied=false`、`recoveryRequired=true` 和分類原因（`tab_missing`／`tab_kind_conflict`／`transfer_receipt_missing`）；不把「不存在」當安全重試或原 Host 死亡證明。新增同一原 Session 活 PTY 回歸、含缺列與擦除收據兩種狀態；TDD **原版 1 fail → 修改版 1 pass／0 fail（exit 0）**。Dart Client 不再誤稱缺收據一定是已持久化成功，保留未知 Outcome 與原 Journal。沒有解鎖、Remint、Terminate 原 PTY 的新權限。
- **Gate B／Lock 世代唯讀診斷**：既有 Lock UUID `processEpoch` 現可從 `inspectRecovery` 和整份 `inspectRecoveryInventory` 讀出；無效 PID／非 UTC 欄位仍拒絕成為可信 metadata。這是人工比對不同 App 世代的**線索而不是 OS/Host Liveness 或 Unlock 認證**。原 `loadPending` 鎖定保持、無自動解鎖和刪檔。最新 3 項 Flutter Journal 回歸 **3 pass／0 fail**。
- **Gate C／已完成的定向檢查**：最新三組 Host Client／PTY Session／Journal Flutter **86 pass／0 fail、exit 0**；`dart tool/quality/check_max_lines.dart --roots lib/src/features/workbench/infra` **exit 0、零超限**；全專案 `dart tool/quality/check_max_lines.dart` **exit 1、134 個超限檔案**，沒有更新 baseline；`git diff --check` **exit 0**，Dart 格式已核對。全域品質紅燈不能被局部通過覆蓋。
- **本輪完整 Host／靜態分析**：Windows `cargo test --locked -p alera-cli --bin alera -- --test-threads=2` **exit 0，1,160 passed／0 failed／6 ignored（測試執行 262.61 秒）**；改動相關檔案的 `dart analyze` **exit 0、無 error／warning，有 3 筆既有 `prefer_initializing_formals` info**。本輪修改 Rust 檔案的 `rustfmt --check` exit 0；全域 `cargo fmt --all -- --check` exit 1 含非本輪修改的 `rust/src/api/git_status_tests.rs` 既有格式差異，不能算全域綠燈。macOS、雙實體手機、多原生視窗／多 App 與 Host OS Process Crash 注入 E2E 尚未取得完整通過證據。

**固定 Gate 決策：A／B／C 仍 OFF。** A 尚缺跨程序 durable Last-view／原 Host 世代權威與 Crash 後確認；B 尚缺雙方 Host＋OS attestation、使用者明確授權、永久 Audit 與可驗證安全人工復原 UI；C 尚缺最新完整桌面＋macOS＋雙實體手機及原生跨程序 E2E 與全專案品質債收斂。任何未知狀態繼續 Fail-closed，保留原 PTY、Tab、轉移 Receipt 和 Journal；不將 PID、App Epoch、Local Archive 當刪除／解鎖授權。

## 2026-10-31 逐項收斂最新交接：Infra 行數品質通過、原 PTY 遺失收據防護紅轉綠

**本節為最新入口；歷史節保留當時的失敗數以便追蹤，不能用較早日期的紅燈覆蓋本節已完成的定向修正。** 整合分支 `refactor/tdd-integrate-20261029`，隔離 Worktree `.worktrees/refactor-tdd-integrate-20261029`；來源 HEAD 依序 `28f847e20` → `a43e7ea66` → `ea76fa062` → `2d00d0933` → `96e9a4761`。本文件提交後以實際 Git HEAD 為準。原始 dirty `main` 不動，`tab.transfer.capabilities.supported=false`，**無合併／Push／Release**。

- **Gate C／Journal 模組化，已提交 `a43e7ea66`**：原本 900 行的持久化／唯讀證據表拆成 **482 + 240 + 187 行**三份 Dart part。原鎖檔 fail-closed、身分檢查、人工診斷 API 不變，Journal 3 項 Flutter 安全回歸 **3 pass／0 fail**。
- **Gate C／PTY 附著與重連模組化，已提交 `ea76fa062`**：PTY Session 主檔 592→**496 行**，抽出 101 行的 attach/reattach helper，保留原 Session、View Lease、輸出游標、拒絕 Remint 行為；四組定向 Flutter **182 pass／0 fail／2 skipped**。
- **Gate C／Host Client 連線與請求模組化，已提交 `2d00d0933`**：Host Client 主檔 886→**459 行**，新增 145/197/114 行的 Routing、Connection、Message Dispatch Dart part，保留原 Socket Hello、已認證資料流、Runtime 設定 Revision CAS、Terminate Mutation ID 與原有 Mixin 覆寫介面。五組定向 Flutter **238 pass／0 fail／2 skipped**。**`dart tool/quality/check_max_lines.dart --roots lib/src/features/workbench/infra`：exit 0、0 違規，沒有更新 baseline。** 全專案 Max-lines 本輪重跑 `dart tool/quality/check_max_lines.dart` **exit 1、134 個超限檔案**（先前 137、減少 3 個）；其他品質債仍待處置，不把局部通過宣稱成全域通過。
- **Gate A／原 Host 程序內轉移 PTY 缺收據防護，已提交 `96e9a4761`**：在原 Session 加入僅當前 Host 生效的 `moved_workspace_fence`，成功 Commit Transfer 後保留來源與目的 Workspace ID。若原 PTY 仍活、但 SQLite Tab 整列遺失或遭替換為「未轉移」無收據內容，拒絕 `terminate`、`createOrAttach`、來源／目的 Workspace 隱含 `Sleep/cleanup`，不清 Lock、不殺原 PTY、不重建 Shell。**TDD 原版 1 fail（缺收據卻允許終止）→ 修正後 1 pass；實際註冊兩側 Workspace 並測缺列與擦除收據。** 修正後 Windows `cargo test --locked -p alera-cli --bin alera -- --test-threads=2` **1,160 pass／0 fail／6 ignored，exit 0**，包括既有 Live PTY 跨 Workspace 原生 Actor Case **pass**。
- **Gate C／前一輪另外已完成**：兩張 Golden 以逐像素核對後定向重建（82／75 像素，未放寬像素比較）；Golden＋Host Client 指定測試 **60 pass／0 fail**；Windows Flutter Widget 全套 **1,071 pass／0 fail／3 skipped、exit 0**；Updater Privacy 模式 **20 pass／0 fail**。這些結果在本輪 Dart/Rust 模組化**之前**取得；目前沒有宣稱最新 HEAD 已完成整份 Desktop Flutter 全套。

**固定門檻仍未全通過**：**A** 的跨原生視窗／多 App／雙手機的持久 Last-view 權威、不同 Host 世代與 OS Process Crash 的確定性安全處置仍未完成；本輪的 `moved_workspace_fence` 只是同一個活 Host 的保守證據，不能證明其他 Host 已死。**B** 的 Exited PTY／stale Journal Lock 仍缺雙方 Host+OS attestation、使用者明確授權、完整永久 Audit 及可驗收的人工安全復原 UI；不允許自動解鎖或用 PID 推論原 PTY 不在。**C** 仍缺最新整合版本的完整 Desktop／macOS／兩台實體手機 Native E2E 與全專案 max-lines 收斂。任何一項未通過均維持 `supported=false`，不合併 dirty `main` 或發布。

## 2026-10-31 固定 Gate 逐項驗收：Updater 完成、Golden 根因量測、PTY Receipt 安全補強

**本節從乾淨整合 `7bb5a13d2` 接續；程式碼來源提交依序為 Updater `fcbce650f`、Host Receipt `ba866b6fb`、Journal Lock `74e33f6a4`；本文件提交後以 Git HEAD 為準。仍只採 A/B/C 三 Gate，不新增階段。`supported=false`、不合併/Push/Release dirty `main`。**

- **C／Updater：已修復並提交 `fcbce650f`。** 正式 Privacy Portable Build 透過預設 `updateDiscoveryAllowedProvider=false` 先拒絕網路查詢；既有 UpdateController Fake 測試明確覆寫為 true 測試狀態流程，新 Privacy Test 斷言未呼叫 Service。`flutter test --no-pub --concurrency=2 --reporter=expanded test/unit/update_controller_test.dart test/widget/update_settings_section_test.dart` 實得 **20 pass／0 fail**，`dart format` 與 Git whitespace 檢查通過。
- **C／Golden：2 例仍未通過。** 對既有參考與測試影像逐像素比對：Core Controls **82/635700** pixels、PR Stack Dialog **75/756000** pixels 有差異，集中於 1px 文字遮罩/控制項邊界，尚不能據此推定無回歸。嘗試在 Golden Harness 明確載入 bundled Inter 後仍為完全相同 82／75 差異，已撤回該實驗；**未更新 Golden 基準、未放寬比較器**。
- **C／max-lines：`dart tool/quality/check_max_lines.dart` 實際 exit 1、137 個超出 500 行或基準容量的檔案；未更新 baseline。** 這是全專案現有品質債，不能當作 T2 通過證據。
- **B／Journal Lock 診斷 `74e33f6a4`：** 新增唯讀的錯誤 PID、非 UTC 時間與有效 UTC 鎖三種測試。前兩項在原程式重現 **0 pass／2 fail**，嚴格檢查正 PID 與具 UTC 時區的時間戳後 **3 pass／0 fail**；正常身分也**不**允許無 Host attestation 自動解鎖，檔案保持原樣，`loadPending` 維持阻擋。這僅提高人工證據可靠度，尚非合法的 Host/OS 簽核、安全人工解除或復原 UI。
- **A／原 PTY 收據防護 `ba866b6fb`：** 新增原 Host 世代 UUID、收據持久 Workspace owner、相異 from/to、Host 觀察退出時間有效性回歸，原始程式 **1 fail（錯誤 owner）** → 補強後同一 Test **1 pass**。修正後 Windows **完整 Host** `cargo test --locked -p alera-cli --bin alera -- --test-threads=2` **1,159 pass／0 fail／6 ignored**，包括原有 Live PTY 轉移 Case **pass**。本防護只是 fail-closed Receipt 正規性，不是 durable 跨程序 Last-view、原 Host Crash 後可回復的 PTY 或安全 stale Journal Lock 解鎖。

**A/B/C Release Gate 仍 OFF**：待權威跨程序／雙手機／Host Crash、使用者授權的安全復原及全部原生 E2E/回歸完成後再討論啟用。未通過前保留原收據、原 PTY 證據與 Journal，禁止推論失蹤 Host 已死並自動清理。

## 2026-10-31 接續：Gate C 繁簡日靜態鍵補齊、Updater 隱私衝突重現（Gate 保持 OFF）

**最新整合程式碼 HEAD `21e000998`**（本文件提交後以 Git HEAD 為準），仍限定原本 **A／B／C 三項固定門檻**，不新增 D 編號。原 dirty `main`（`3d232acff`）既有修改與未追蹤項目保持不動；無 merge／push／release，`tab.transfer.capabilities.supported=false` 不變。

- **C／語系修復已實作並提交**：`21e000998` 在四個獨立 Dart part 補上簡中 **215** 與日文 **218** 個缺漏的實際翻譯，仍由 `alera_localizations.dart` 組合；未塞入英文占位、未放寬鍵集合測試。初次重現 `app_localizations_zh_cn_ja_test.dart` **28 pass／2 fail**（簡中／日文靜態鍵 parity）；修復後聯合 `app_localizations_test.dart` **72 pass／0 fail**。新增檔案與總表 `dart format` exit 0；`git diff --check`、`git diff --cached --check` exit 0。
- **C／Updater 舊版測試仍紅**：`flutter test --no-pub --concurrency=2 test/unit/update_controller_test.dart test/widget/update_settings_section_test.dart`，Controller **13 fail**（期待在線更新查詢，現行 Privacy Portable Build 的 `checkForUpdates` 明確禁止線上查詢），兩者聯合 **6 pass**。此衝突**不是 T2 PTY 損毀**，也不能透過啟用隱私版線上更新「修測試」；待獨立修復產品策略／測試契約。其他原記錄 Golden／max-lines／原生 E2E 未本輪重跑，不宣稱已綠。
- **A/B 未變**：持久跨原生視窗／多 App／雙手機的 Last-view 權威、SQLite Commit 後原 PTY Actor 失聯 Crash 定論、原 Host OS process generation 驗證、Exited PTY／stale Journal Lock 的使用者授權與人工安全復原／Audit 都仍缺正式 E2E／簽核。**任何不確定狀態保持 Fail-closed、保留 Receipt/Journal，不自動解鎖、殺 PTY 或退休 Tab。**

**決策**：Gate **A、B、C 均未通過**，`supported=false`，不合併 dirty `main`；下一步只針對既有 A/B/C 驗收證據及其明確失敗項目收斂。

## 本輪新驗收：Host 完整測試全通過，三個啟用門檻仍未全部完成（2026-10-30）

**最新整合基準**：`refactor/tdd-integrate-20261029`、`.worktrees/refactor-tdd-integrate-20261029`。本輪從 clean `d73ed7fad` 延續，隔離來源 Worktree 三線修正、按序 cherry-pick；程式碼提交後 HEAD **`742e0d556`**（本節文件 Commit 後需用 Git 重新確認）。原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0`、原有未追蹤測試/歷史/xterm 保留，**未合併／Push／Release／改 Gate**。`rust/alera-cli/src/terminal_host/server/requests.rs` 的 `tab.transfer.capabilities.supported=false` 仍 OFF。

**本輪真正取得的新通過證據（以完成的命令為準）**：
- Windows Rust **完整 Host** `cargo test --locked -p alera-cli --bin alera -- --test-threads=2`：**1,158 pass／0 fail／6 ignored**；前一輪兩項失敗（舊 `terminal.restart` 文字、Mobile Hello Fixture 缺 `terminalNamedViewLeaseV1`）已在新版本真實通過。該次 Host 全套檢查在新增下一筆 Crash 回歸測試前完成。
- 桌面 T2 相關 `flutter test --no-pub --concurrency=2` 定向 9 個測試檔：**53 pass／0 fail**，包括 Tab 拖曳、Layout、Tab Close、Workbench Controller Provider Boundary、Sidebar Boundary、Language Intelligence Architecture、Settings Ownership/Search Catalog 與 Host Wire Fixture Socket Cleanup。
- 本輪新增的 Rust Host **原 PTY 遺失後 fail-closed** Actor Case `existing_pty_moves_atomically_but_unknown_second_view_rejects_it` 已在最新程式碼提交 `742e0d556` 上重新編譯、實際執行：**1 pass／0 fail**；檢查安全移動後移除 original Session 仍保留 Transfer Receipt，Status 回 `pty_missing`／`recoveryRequired`，不許重新 Attach/Remint。**不代表舊 Host OS Process 已證實停止，也不授權手動解除 Journal。**
- `dart analyze` Desktop App 組裝後 `language_intelligence_providers.dart`／Controller Internals／Tabs：**No issues found**。
- 上一輪已確認：原 Host Actor 單 PTY 轉移 **1 pass**、Rust Store 原收據與安全退休 **3 pass**、Mobile Flutter **473 pass／4 skipped／0 fail**，必要 Rust `cargo check` 通過。

**本輪新增六筆安全／回歸整合提交**：`f076157f5` 補齊第一批 29 個簡中/日文 Language Intelligence Key、`7f3c7d50c` 透過 Internals Narrow Getter 組裝 T2 Provider、`faad1ec0e` 將 Language Intelligence Riverpod 與生成碼搬到 App Root、`461549354` 校正 Host fail-closed Restart 及 Mobile Hello 能力 Fixture、`97307cf88` 將 Git Auto-fetch 欄位列為 Local Only 並修 Host Wire Fixture Socket Dispose 併發變更、`742e0d556` 新增模擬已提交跨 Workspace Transfer 後**原 PTY Session 消失**的 Actor 回歸：不能因持久 Receipt/Layouts 就宣稱成功或啟動替代 PTY，應保留原資料與 Recovery Required。新測試已在該 HEAD **1 pass／0 fail**；完整 Host 1,158 pass 是同一輪修改在增補此新測試**之前**的結果，不宣稱最後 Commit 也已重跑整套。

**仍不可開啟 `supported:true` 或直接合併**：
1. **A：持久 Host/PTY/View Ownership 與 Crash**：跨 Win/macOS 原生視窗、多 App Process／手機及 Host process Generation 的耐久 Last-view 權威、原 PTY 無法尋回後的確定性處置仍未實作；本輪有更多「保留資料、拒絕重建」測試，**不是**完整自動復原能力。
2. **B：人工復原**：Host 原 PTY 已 Exit／失蹤、Journal stale Lock 的 OS+Host attestation／使用者意圖／永久 Audit 及安全解鎖 UX 仍不足；現行安全策略維持未解而不刪資料。
3. **C：完整跨平台／應用回歸**：本輪早期 Desktop 全套 `flutter test --no-pub --concurrency=2` 在 Windows 600 秒上限附近約 **3,608 pass／31 skipped／26 fail**，非完整通過。定向回歸已修復上述 T2/架構/設定問題，但 CN/JA 靜態語系總表仍分別缺 **217／220** 個相較繁中鍵（前次修 29 個後仍有差額）；另有 Golden 微像素差、max-lines 52 檔、舊 Updater Privacy Build 測試期待等**非 T2** 回歸未處置。不能用拋棄測試／虛假填英文翻譯／硬更新 Golden 或 Ratchet 取得假全綠。缺 macOS/實體雙手機/Windows 多 OS Process Crash-injection E2E。

**決策**：不再新增 D 編號階段，僅追 A/B/C Release Gate。Rust Host 與 T2 定向測試已有關鍵突破；但直到 Host 崩潰/多程序恢復和合法手動退出流程可證明安全，以及必要原生 E2E 達標前，繼續拒絕啟用。原 dirty `main` 不動。

## 最新實際驗收與整合交接（2026-10-30；Gate 仍 OFF）

**本次應使用此節作為最新入口，而不是下方早期的「未允許執行測試」限制摘要。使用者現已要求完成合併與啟用，明確授權必要編譯／測試；但未授權遺失 dirty `main` 既有修改或跳過安全驗收。** 依新要求已在隔離整合工作樹實際跑測試；Git HEAD 需以 `git log -1` 重新核對。

**已驗證通過：** Windows Rust `cargo check --locked -p alera-cli`；Rust Store 原收據、退出證據和單交易退休的新增 3 項測試；`alera-cli` Actor 真正跨 Workspace 單 Session 原 PTY 測試 **1 pass**（未知第二 View 阻擋、解除後同一 PTY 遷移、雙 Layout/Receipt）；手機完整 Flutter **473 pass／4 skip／0 fail**，手機 T2 Dart Analyzer 0 issue；桌面拖曳／Layout／Tab Close **29 pass**；桌面 Terminal 重連／Controller／清理 **162 pass**；RustProcessRunner/Detached Process Conformance **41 pass**；桌面 T2 定向 Dart Analyzer exit0（少量 style info）。

**最新隔離整合修正：** Windows Git Hosted Review `file:///C:/` Test URL、更強的 Store 退休測試、Mobile 具名 View Lease 介面型別、Desktop Optional Recovery/Scoped Tab/PTY View Lease 型別與顯示 Workspace ID、Host Actor 測試 stale-owner 與唯一 PTY 回歸、Managed LSP 執行改用 Rust ProcessRunner 避免 Dart 的 PTY `ECHILD`、舊 Tab 消失不可清除原 Codex Session 的安全測試、Host 異步 `tab.remove` 測試處理完整 Actor preflight／completion 兩階段；設定頁三語系翻譯與 Git History 輔助測試檔命名。請以最近 Git log 確認實際 commit（本節撰寫時最近 `f296cd58e`）。

**仍不可啟用／合併的確切阻塞：**
- 桌面完整 Flutter `flutter test --no-pub --concurrency=2` 在本輪已實測約 **4564 pass、34 skipped、34 failed**；含 Golden 兩張小像素差、設定／語系及更新器／架構／Settings 欄位檢查等多項非 T2 回歸。T2 相關 Terminal 清理四項已後續定向修正且 162 pass，LSP process 41 pass；全套**尚未**重跑通過。三語系定向補強後 `app_localizations_zh_cn_ja_test` 還有此前累積的 Language Intelligence Static Key parity 失敗，不可把後續碼版宣稱全綠。
- Host `cargo test --locked -p alera-cli --bin alera` 執行到 `stale_tab_removal_does_not_depend_on_remote_cleanup` 時舊測試卡住，已修正舊測試假定只需處理一個 Actor 訊息的失誤（現在兩階段 preflight + background completion）。**修正版**重跑 Windows MSVC Debug Test binary 編譯時間極長／曾逾時，目前沒有該例與整套 Host tests 最新成功證據；此前原始 PTY 跨 Workspace 精準 Actor 測試 **1 pass** 屬另一獨立案例。
- 原有 Gate A/B 尚未完成：Host 重新啟動／原 PTY 消失的權威 outcome、跨 Native Window／App Process／Phone durable Last-view 身分、已退出 PTY 的合法人工退休及 Journal stale Lock 的 OS/Host 認證稽核。不得僅因單窗/單 Host 模擬測試成功就開啟所有裝置拖放。
- Mac/實體雙手機 E2E 不可在目前 Windows 工作環境證實；真實 Windows 跨原生進程與 Host Crash injection 亦無完成驗收。原 dirty main 尚未合併；`rust/alera-cli/src/terminal_host/server/requests.rs` **`supported=false` 保持**。無 Push/Release。

**具體下一行動，不再新增 D 編號**：① 排除 Host MSVC Test binary build blocker、取得所有 Host 測試 pass；② 修完整桌面 34 項殘留回歸，重新執行全套；③ Host/Journal/Live View 確定性 Crash/manual recovery 和跨實際 Win/Mac/Phone E2E；④ 只有 A/B/C 所有證據通過時才由乾淨整合安全合併 dirty main（保留其未提交內容）並啟用 capability，否則維持 OFF。

> **狀態：收斂開發，尚未可啟用。** 不再依照 D18、D19、D20 無限分批。現在的任務是完成確定的可安全修改項目，然後按本文件的**有限驗收清單**決定能否開 Gate。沒有編譯與真實 OS/Phone E2E 就不宣稱完成，不為了趕完成而刪原 PTY 或解鎖 Journal。

## 整合分支與原限制

- 唯一整合分支：`refactor/tdd-integrate-20261029`（`.worktrees/refactor-tdd-integrate-20261029`）。本輪起點：D17 **`7cac43d53a81198fdc10d601f188b02da45cb7de`**。
- 本輪兩條隔離來源線：Store **`d479429e7`** → 整合 **`09c8aba3a`**，Hash 編碼靜態補正 **`99cbe4a4c`** → **`46794ddfe`**，Host 停止程序前再檢查來源 Scope **`7c7689675`** → **`185510980`**；Journal **`be6d1922a`** → 整合 **`d12ca2e69`**。本文件提交後 HEAD 再前進，接手先以 `git log -1` 查證。
- 原始 dirty `main` HEAD `3d232acff9822154c68049ee63c57ed5966b84e0` 及所有原有 dirty files 不動（僅既定 session checkpoint 會依工作流程寫歷史）。
- `rust/alera-cli/src/terminal_host/server/requests.rs` 的 `tab.transfer.capabilities.supported=false`；無自動開放跨 Workspace 拖曳、無 Push 或 Release。
- 本輪依先前限制，**只使用 Rust/Dart format、Git whitespace checks、Git commit/cherry-pick**。沒有執行或新增 Cargo/Flutter Tests、Cargo check、Flutter analyzer、Build、Win/macOS/Phone 原生 E2E。

## 真正收尾且已整合的可檢查項目

| 邊界 | 實際處理 | 尚不可推論 |
| --- | --- | --- |
| 普通 Store 單 Tab 刪除 | `RuntimeStore.remove_workspace_tab` 在**同一 SQLite 交易**內確認該 Tab 沒有 `lastTerminalTabTransfer` 或 `lastTerminalTabExit` Key（包含 JSON null／畸形），否則拒絕；普通非 transferred Tab 照舊可刪 | 原流程 `remove_workspace_tab` 不再是移轉 Tab 的合法刪除 API |
| Workspace/Project 級聯／Sleep | `remove_project`、`remove_workspace`、`remove_workspace_tabs_for_workspace`、`sleep_workspace` 均在各自的**實際 SQLite 刪除交易**內確認不存在無法安全丟棄的 transferred Evidence；也掃描**跨到其他 Workspace**但還把即將刪除的 Workspace 記作來源／執行 scope 的原 Receipt。**Host 在不可逆程序關閉之前也做相同來源 Scope 唯讀預檢**，刪除交易再次重驗。遇到孤兒／壞收據 fail closed | 不能以 Target 沒有此 Tab 為由任意移除 Source Workspace；此防線可能令無法歸屬的壞 Receipt 對 Workspace 清理保持阻塞，需人工處理 |
| 主動關閉的唯一原 View（尚須實測） | 新 `remove_transferred_terminal_after_verified_live_view` 只供 Actor 已核對**原始活 PTY + 最新 Receipt + 唯一 View Token**後的序列化 `tab.remove`。Store 再核對 original Host Generation/Session/PTY Token、owner、兩側完整持久 Layout/Tab registry、Exit Marker **不存在**。同一交易：刪 Tab → 從 Target Pane 移除並必要時修剪 split tree → 寫入 `terminalTabRetirementAudits` 永久稽核（只存 SHA256 View Token，不存 Bearer Secret）。任一 CAS/版面失敗整筆回滾 | SQLite 不能自己證明其他 Host Process/OS View 的生命狀態；目前 Actor 的 per-View 判定仍依原 Host in-memory 證據；**不能**拿這條途徑假裝 Host Crash 後原 PTY 仍活著 |
| Journal 跨程序 Lock | Lock 版本更新為 `2`，新增 App Process 隨機 `processEpoch`，正常釋放仍需同 nonce、Tab Hash、PID、Process Epoch；不會因 PID 被回收重用就視為原 App。舊格式/中斷 Lock 只在 Inspect 顯示異常並保留 | `processEpoch` **不是**跨 OS Process liveness check，也**不是**安全 unlock 權限；不提供 stale lock 自動刪除 |

以上在 D1–D17 的已整合安全守衛基礎上完成，含轉移單筆兩 Layout CAS、原 PTY Token、Host 狀態唯讀核實、Mobile 原 View Revision/Host Client 检查、Journal Pending/Archive Dossier。

## 目前仍未完成、因此不能開 Gate 的**有限**驗收門檻

| 必要門檻 | 可接受的完成證據 | 現狀 |
| --- | --- | --- |
| **A. 崩潰及跨程序/視圖權威** | Windows/macOS 不同原生視窗／不同 App Process、兩支手機同時附著／解除、Host 重啟／崩潰後，能對原 Host Generation 和最後 View 做**安全且一致**的判定。當原 PTY 確實無法恢復時，顯示清楚人工確認與安全退休／重新開 Shell 的選項，**不冒用原 PTY 世代** | 未完成跨程序 durable registry、Host Crash definite resolution；現有邏輯採拒絕誤刪 |
| **B. 已退出 PTY / Journal 證據安全人工復原** | 不僅有唯讀原 Exit+兩 Layout 同快照，還有使用者意圖、Host/OS Process Incarnation 和已確認 View 的完整驗證；可安全解鎖 stale Journal 且保留 Host 權威／本地稽核，在未知狀態不自動清理 | 原 Host 正常活 PTY 的最後 View Close 已有專用 CAS 交易；**已退出／Host 已崩潰**的可靠人工恢復與 stale Lock unlock **尚未實作** |
| **C. 靜態／功能／原生整合驗收** | `cargo check`、Rust/Flutter tests、Flutter analyzer、Windows/macOS/手機/跨多視窗 E2E、反覆失敗重試/DB Crash injection 全部有可重現結果；Gate ON 前確認原有 Terminal、Tab 重新排序、Detach、Diff、其他 Store 刪除流程沒有退化 | **先前明確限制不允許執行**，因此無任何通過證據；不會聲稱測試成功 |

### 決策

- **目前可以交付的**：整合分支上的 Store 安全處置、原 View 主動關閉專用交易與稽核、Journal Lock 世代保護及既有 D1–D17 功能安全補強；Git whitespace／格式層級驗證。
- **目前不可以宣告的**：跨 Workspace Terminal 移動已通過最終 E2E、已能跨 Host Process Crash 保留原 PTY、所有 View 的 durable Last-view、Stale Lock 已可安全人工解鎖、可合併 main／發布。
- **不再新增新編號階段**：後續只用上面 A/B/C 三項**明確的 Release Gate**關閉剩餘工作。若持續限制測試／編譯／實機操作，C 無法達標，功能必須保持 `supported=false`。
