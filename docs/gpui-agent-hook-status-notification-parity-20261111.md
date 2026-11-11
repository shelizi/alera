# Alera GPUI Agent Hook 狀態／桌面通知對齊與 TDD 追蹤（2026-11-11）

> **此檔是唯一進度追蹤表。** 勾選必須附上 RED→GREEN 測試、程式 Commit 與實際驗證結果；未執行的 GUI／Host 測試不可標成通過。
>
> 狀態：**IN PROGRESS／不可發布**。分支：`feat/gpui-agent-hook-parity-20261111`，工作樹：`.worktrees/feat-gpui-agent-hook-parity-20261111`，基準 `c1fb16e75`（GPUI 整合分支）。
> 保護：不改 Classic `main`、不取代／停止既有 Runtime Host／PTY、不對使用者 Git 工作樹執行寫入、不在未完成 Gate 前 push／release／合併。

## 需求與 Classic 真相

- Classic 有 `working / waiting / blocked / done` 四態；中斷另有 `interrupted` 標記，狀態必須綁定 Host Terminal Session／Workspace／Tab／Agent Type。
- Classic 由 `agentPresence.list` + `agentPresenceChanged`（變更 delta）同步 Host；重連立即快照、丟棄過期快照，不能讓過期狀態覆蓋新值。
- Classic 桌面通知：`waiting / blocked`，可設定啟用 `done`；同 Session、同狀態 60 秒冷卻，3 秒 debounce、10 秒最大合併延遲；批次合併，不對初始化歷史狀態洗版。
- Classic 通知點擊：啟動視窗 → 定位 Project/Workspace/Tab → 焦點回到對應 Terminal。必須先重新驗證 Host 身分，防止過期 Session、重用 handle、切換 Workspace 錯誤導航。
- GPUI 目前 `TerminalSession` 只保存 `running` 與 `agent_type`，`agentState` 在 Host `terminal list` 回覆中已存在但被丟棄；GPUI Settings 中部分 Hook 按鈕仍屬 Preview，通知偏好有共用欄位但 GPUI 尚無通知執行者。
- 現有 Rust Host 有認證 Hook Receiver、四態正規化、`agentPresence.list` 和 `agentPresenceChanged`。**不得另外安裝第二套相互衝突的 Hook Receiver。**

### 現況判讀

| 功能 | Classic | GPUI 基線 | 目標 |
|---|---|---|---|
| Hook 與四態 | Host / Controller | 顯示 running ○/●，未投影四態 | 共用 Host 四態和顯示 |
| 左欄及 Tab | StatusDot + Tooltip | 非四態 | 精確附著到 Session／Tab 身分 |
| 事件與重連 | list + delta + generation | 尚無 Presence 訂閱 | 相同時序、拒絕 stale |
| 通知判斷 | 狀態選擇、去重、合併 | 尚無 | 不重複、不漏掉真新事件 |
| OS 通知／點擊 | Windows/macOS/Linux + Navigation | 尚無 | UI 與跨 Workspace 正確導航 |
| Settings | Hook 開關／通知開關 | 共用偏好僅部分介面 | 真實可讀寫，無假成功 |

## TDD 開發階段（每次修改此表）

| ID | 階段 | 先做的 RED 測試 | GREEN 定義 | 目前 | Commit / 證據 |
|---|---|---|---|---|---|
| H0 | 建立規格／隔離工作樹 | N/A | 檔案落地、基準與保護驗證 | DONE | 規格首版，隔離 feature worktree |
| H1 | 四態資料模型／安全解析 | Host 回傳四態、缺狀態／未知狀態、Session 停止、錯誤 Workspace/Tab、身分重用 | 顯示與解析四態；無法確證時 unknown，絕不把 running 當 working | **PARTIAL：解析 GREEN；Host 身分競態待 H3** | `terminal.rs` RED=E0609/E0433；全套 Library **392/392 PASS** |
| H2 | GPUI 版面渲染 | Tab 與左欄顏色、Tooltip、等待／阻塞、重繪後復原 | 四態顯示／無重疊、其他 Tab 不變 | **PARTIAL：Tab 投影、狀態變化即時重繪、色彩 GREEN；左欄/Tooltip/GUI 尚未驗收** | Library **403/403 PASS**、Native Binary **21/21 PASS**、Native Check exit 0 |
| H3 | Host Presence 同步 | 首次快照、delta、亂序、重連、失聯、取消、切換 Workspace／世代 | 唯一 Host 真相、增量正確、過期請求拒絕 | **PARTIAL：安全 Host delta push 接線、過期事件／舊快照競態護欄、舊版 invalidation fallback 已 GREEN；隔離 Host 端到端和全域無預覽訂閱未驗收** | CLI stream **3/3 PASS**；Native Library **407/407 PASS**、Native Binary **21/21 PASS** |
| H4 | 通知規則 | 60s cooldown、3s/10s coalesce、done toggle、初始化／重連不補發、關閉 Session 去重 | 純 Rust 可重現與無資料競態 | **PARTIAL：純 Rust 決策 8/8 PASS，已接進 H5 Native 通知排程；真實 OS 顯示仍待 H5 實機驗收** | `agent_status_notifications.rs`：新回合 RED assertion→GREEN，Library **413/413 PASS** |
| H5 | Windows OS 通知／點擊 | 許可、關閉通知、同時兩版去重、Windows AppId 不污染 Classic、點通知定位正確 Pane/Tab | 具體隔離桌面通知可見且導航安全 | **PARTIAL：WinRT Toast adapter + Host 規則接線 + 本工作區點擊安全路由已通過程式驗證；隔離 Windows GUI、跨工作區導航與 Classic/GPUI 自動通知仲裁仍 TODO，正常模式預設不投遞** | H5 RED E0433/E0425 → GREEN；Library **413/413 PASS**，Native Binary **21/21 PASS**，Native cargo check exit 0 |
| H6 | 設定與跨版本 | Host 設定讀寫 CAS、停用、舊 Host、離線錯誤與重試 | 設定真實生效、Classic / GPUI 行為一致 | **PARTIAL：Host operational `agentStatusHooks` 具版本化唯讀快照、依 Provider 的 Native 通知閘門、單一 Provider 的 `expectedRevision` CAS、Codex／Claude GPUI 實際開關 UI。完整 Agent 清單與實機操作、舊 Host 相容回退、Classic 雙開通知權仲裁仍未完成** | **H6-5：GPUI Library 417/417 PASS、Native `cargo check` PASS、CLI `cargo check` PASS**；**H6-6：獨立且使用正式 CLI CAS 原始碼的 8/8 測試 PASS**；Clap 子命令及 Host 交易測試仍未在大型 Windows CLI 測試版執行；正式 Host／GUI CAS 互動 NOT RUN |
| H7 | 完整驗收 Gate | Windows Native Build、Rust 全套、隔離 Host 真 Hook、四態點擊、兩版同時使用、DPI/IME | 逐項 GUI PASS；`supported` 才能另案評估 | TODO | — |

### 必測邊界

1. Host 四態：`Working → Waiting → Working → Blocked → Done`；`interrupted`；同 Session 新回合；Agent 轉型。
2. Session identity：handle 與 `workspace_id`、`tab_id` 完全相符才更新；被刪除、移動、重用不可用過期通知操作另一個 Session。
3. 僅有 `running=true` 不可推論 `working`。未取得 Presence 應明確顯示未知或不顯示四態。
4. 快照與增量：斷線→重連、延遲／反向到達、切換 Workspace、關閉 pane、Close Others。
5. 多客戶端：Classic 和 GPUI 同時存在時禁止 OS 通知雙重投遞；Host 更新不影響兩邊顯示一致性。
6. 通知：首份快照不洗版；3 秒短暫抖動不連續彈窗；使用者關閉設定應立即停止投遞；點擊時再確認身份。
7. 資安：Host Token 不進 UI、RPC `--no-start`、不額外開放 Hook TCP 端口、不洩露 Prompt/ToolInput 到通知；有界佇列和輸入校驗。

## 每輪紀錄

| 日期／輪次 | 修改 | RED | GREEN／Build | GUI／Host | 下一步 |
|---|---|---|---|---|---|
| 2026-11-11 初始 | 建立功能基準與規格 | 未執行 | 未執行 | 未執行 | H1 模型測試起步 |
| 2026-11-11 H1-1 | `TerminalSession` 保留 Host `agentState` 四態及 `interrupted`，未證實的狀態保持 None | `cargo test --lib host_agent_status_ --no-default-features`：編譯失敗 E0609/E0433（7 診斷），符合預期 RED | 指定 **1/1 PASS**，完整 `cargo test --offline --locked --no-default-features --lib -j 2` **392/392 PASS**（含 unknown 退化測試） | 未跑最新 GUI | H2 Tab 狀態標示與狀態變換 |
| 2026-11-11 H2-1 | `WorkbenchTab` 增加 Host 狀態和 interrupted，左右 Pane 的 Chip 根據 Classic info/warning/error/success 顯示 | `agent_hook_status_changes_update_the_same_tab_without_claiming_pty_ownership` 編譯 RED E0609 | `cargo test --offline --locked --no-default-features --lib -j 2` **394/394 PASS**；`cargo check --offline --locked --bin alera-gpui-desktop -j 2` **exit 0** | 未實際 Windows GUI 點擊；左側 Project/Worktree 狀態尚未實作 | H3 Host 動態更新與 H4 通知判斷 |
| 2026-11-11 H4-1 | 新增純 Rust `AgentNotificationTracker`／`AgentNoticeCoalescer`，匹配 Classic 60 秒冷卻、3/10 秒合併；初始快照忽略、受控 Done、Session 重用拒絕、停用設定不補送 | 新 5 個測試因 `AgentNotificationTracker`／`AgentNoticeCoalescer` 不存在編譯 RED E0433 | 先修 Hash key 型別，再跑專項 **5/5 PASS**，全套 Library **399/399 PASS** | **尚未連接 OS 通知**；此階段只有決策層，不可宣稱 GPUI 已會跳 Toast | H3 資料驅動與 H5 OS Adapter |
| 2026-11-11 H3-1 | 複用 `terminal list --no-start`，活躍 Agent 由每 15s 改為每 3s 刷新四態；沒有活躍 Agent 保持 15s 間隔；不新增 Host／PTY | `active_agent_status_polls_faster_without_spinning_when_host_is_idle` 編譯 RED E0425 | 指定 **1/1 PASS**，全套 Library **402/402 PASS**；Native `cargo check --offline --locked --bin alera-gpui-desktop -j 2` **exit 0** | 尚未有 Host `agentPresenceChanged` push event；不可宣稱與 Classic 事件時序相同 | 後續實作雙向 Presence push/delta generation、通知投遞 |
| 2026-11-11 H4-2 | 補足連續抖動最長 10 秒強制 flush、等待期間狀態解除可取消排隊的實測 | 既有 H4 RED E0433，另增兩個回歸防再犯 | 新增 2/2 PASS，通知規則合計 **7/7 PASS**、全套 Library **402/402 PASS** | 無 OS 通知啟動證據 | H5 顯示／導覽與跨版本通知仲裁 |
| 2026-11-11 H2/H3-2 | 修正 Host `terminal list` 定期刷新後 Tab 沒有同步重繪四態的缺漏。新增 `WorkbenchTabs::refresh_agent_statuses` 只更新**完全符合 Host session/workspace/tab 身分**的 status/interrupted；不觸碰 Tab 排序、釘選、active 焦點或底層 Host/PTY。Host 列表失敗時清除未確證狀態 | 新測試 `agent_hook_refresh_only_updates_exact_host_metadata_without_stealing_tab_focus` 編譯 RED **E0599** | 指定 1/1 PASS；完整 Library **403/403 PASS**、Native Binary **21/21 PASS**、`cargo check --offline --locked --bin alera-gpui-desktop -j 2` **exit 0** | **仍未實際 Windows UI、Host Push、OS Toast 驗收** | H3 正規 `agentPresenceChanged` delta、H5 Windows Toast |
| 2026-11-11 H3-2 | 使用既有 CLI ↔ Host 認證長連線轉送 `agentPresenceChanged`；CLI 僅放行 session/workspace/tab/agentType/state/time/interrupted，**不轉送 Prompt、ToolInput 等私密字串**。Native 雙重驗證識別與 UTC 時序，delta 僅更新完全相同的執行中 Session；較舊事件／CLI 快照不得覆蓋較新事件；晚到的舊 Inventory 請求依世代捨棄。Host 關閉事件僅有 sessionId 時退回完整清單，不誤刪 recycled handle | Native `native_stream_accepts_only_identity_scoped_agent_presence_changes` 編譯 RED E0433/E0599；`host_presence_delta_rejects_stale_events_and_old_inventory_snapshots` 先寫測試 | Native 專項均 GREEN；跨層 `sanitized_host_push_updates_only_exact_live_agent_without_terminal_observe` PASS；完整 Rust Library **407/407 PASS**，Native Binary **21/21 PASS**、Native `cargo check` exit 0；CLI stream **3/3 PASS**（原 PTY 權限、事件篩選與 Host 保密） | 尚未使用隔離 Host 真 Hook 驗收；事件僅在既有 Native CLI stream 活動時接收；OS Toast 尚未實作 | 隔離 Host delta、H4 通知事件接線 |
| 2026-11-11 H4-3 | 同狀態但不同 `stateStartedAt` 的新 Agent 回合，視為新通知候選；初次時間戳補齊不補發，60 秒同 Session/同狀態冷卻不變 | `same_attention_state_new_agent_turn_is_not_lost_after_cooldown` **RED：0 通知，預期 1** | 指定 1/1 **GREEN**；H4 通知純規則 **8/8 PASS**，完整 Library **407/407 PASS** | 沒有接上 OS Toast／多客戶端通知仲裁 | H5 真實 Windows 通知與點擊 |
| 2026-11-11 H5-1 | 新增 `AgentNotificationDelivery`，每次 Host state delta／清單都累積有界通知候選；1.5s 既有 Housekeeping timer 在 3–10s 規則後投遞。每批投遞前重新核對 Session/Workspace/Tab/Agent type/state、被回復／移除／換身分的事件撤銷；設定停用或沒有唯一 Native 所有權立即清空，不補發歷史通知。Toast 單筆點擊走 GPUI Window deferred callback，再次比對 Host 身分、僅正確的目前工作區可聚焦；其他工作區拒絕誤導覽 | 先寫 `attention_delivery_requires_explicit_native_ownership_and_host_permission`、`resolved_or_recycled_agent_cancels_delivery_even_after_coalescing`、`notice_activation_rechecks_exact_host_identity_before_navigation`：型別／函式不存在，編譯 RED E0433/E0425 | 通知決策新增 **3/3 GREEN**，加上重連 **1/1 PASS**；完整 Library **413/413 PASS**、Native Binary **21/21 PASS**，Native `cargo check --offline --locked --bin alera-gpui-desktop -j 2` exit 0 | **正常／Release 不投遞 Native OS Toast**：Classic 正式通知投遞權未交接。只有 Windows debug + 明確 `ALERA_GPUI_AGENT_TOAST_EXCLUSIVE_TEST=1` 的人工隔離測試入口；此環境變數不是安全的跨客戶端 Host 仲裁 | H5-2 註冊／授權／真實 Toast click；H6 Hook 設定；跨工作區正確導航 |
| 2026-11-11 H5-2 | 以 GPUI 獨立 `NATIVE_WINDOWS_APP_ID`（不同於 Classic 的 Flutter notification AppUserModelID）加入 `windows 0.62.2` WinRT `ToastNotificationManager`、`ToastNotification`、`XmlDocument`，所有 Prompt/ToolInput/Host ID 不進 Toast XML；輸入一律 XML escape、有界 128 筆、群組通知一次顯示；單一通知的 in-process activated callback 僅傳有型別的 Host target 並再驗證 | `windows_toast_xml_has_no_prompt_tool_input_or_unescaped_agent_text`、`toast_only_accepts_one_or_more_bounded_attention_items` 編譯 RED：XML 建構函式不存在 | Toast XML 安全 **2/2 PASS**；WinRT native binary typecheck exit 0；完整 Library **413/413 PASS** | **尚未測 Windows Toast 真實彈出／系統權限／Start Menu shortcut 註冊／點擊 HWND 喚醒**；WinRT API 返回成功不等於實際 UI 已顯示 | 隔離 GUI 與 Classic 雙開仲裁，勿打勾 H5 |
| 2026-11-11 H5-3 | `AgentNotificationDelivery::on_host_reconnected` 要捨棄在途 Toast，不將重連時已有的 Waiting 當成新通知；真正恢復工作又進 Waiting 則可受 60 秒 cooldown 管理 | 補 `reconnect_discards_pending_toasts_and_does_not_replay_waiting_snapshot` 回歸案例 | 指定 1/1 PASS，Library **413/413 PASS**。本輪第一次 Native Binary **21/21 PASS**；追加此 Library-only 測試後的第二次 Native Bin 重跑在連結階段 4 分鐘沒有新輸出，已由本輪控制端停止，**此第二輪 NOT RUN/INCOMPLETE**。Native `cargo check` 仍 PASS | 未實際斷線啟動 Windows Toast；對第二輪中止不虛報成功 | H5 系統／跨版本端到端 Gate，後續 Native 重測 |

| 2026-11-11 H6-1 | Classic 將 `agents.agentStatusNotificationsEnabled` 這種可攜通知偏好與 Host Runtime 的 `agentStatusHooks`（每種 Agent 開關）分開保存。GPUI 原本只讀前者，現在新增 `alera settings --no-start --json agent-hooks-get`：CLI 對 `runtimeSettings.get` 只投影最多 64 組 `agentStatusHooks` boolean，完全捨棄 AI Assist／憑證／其他 Runtime 設定；Native 二次驗證 kind 與兩欄位 schema，失聯、舊 Host/CLI、非法欄位全拒絕。Native 另以 3s（Settings open）／15s（一般）刷新 Hook 快照，再按每個 Provider 的實際啟用狀態決定通知候選；Hook 關閉會清空待通知佇列，不會回補舊 Waiting | CLI `agent_hook_readback_*` 編譯 RED E0425/E0599：缺 projector／subcommand；GPUI `native_hook_settings_snapshot_is_strict_boolean_read_only_and_fail_closed` 編譯 RED E0425：缺 parser；`per_agent_hook_flags_from_host_filter_native_notifications_and_fail_closed` 新增 | Native parser **1/1 PASS**，完整 Rust Library **415/415 PASS**，Native `cargo check --offline --locked --bin alera-gpui-desktop -j 2` **exit 0**；CLI 專測已完成編譯檢查階段但測試版**連結超過 360s 而逾時，NOT VERIFIED**；CLI 普通 `cargo check` 另行驗證 | 目前仍不支援 GPUI Hook switch 寫入 CAS／真正 OS 通知雙客戶端仲裁，預設維持 Native 不投遞 | H6 Runtime Hook 設定可編輯 UI + CAS、H5 正式單一 Toast owner |

| 2026-11-11 H6-2 驗證阻塞 | CLI `alera` 專項測試執行檔建置在 360s 固定期限內未完成，未取得 GREEN／RED 執行結果；改執行 CLI `cargo check --offline --locked -p alera-cli --bin alera -j 2` 後，冷檢查快取 240s 逾時；使用已產生快取再查，持續停在 `aws-lc-sys`、`libgit2-sys` 等 C 原生依賴，人工終止**僅本輪 CLI check 工作**，沒有停用 Host 或 PTY | 不是程式測試斷言失敗，而是編譯資源／期限阻塞 | **GPUI Library 415/415 PASS、Native `cargo check` PASS、CLI 測試與 CLI check 均 NOT VERIFIED**，不得將 CLI 標為通過 | 此功能仍僅在獨立未合併分支，`supported=false` | 下次以可用較長建置資源完成 CLI check/2 個 CLI 專測，並使用隔離 Host 唯讀查詢 |
| 2026-11-11 H6-3 MSVC／上限修正 | Windows 主機安裝 Visual Studio 18 Community 的 `vcvars64.bat` 與 `cl.exe`，舊的遠端 shell 未初始化 MSVC。新增 `scripts/verify-agent-hook-cli-msvc.cmd check\|test`，使用 `vswhere` 探查已安裝 VS，僅在該子程序載入開發者環境，支援原有 `CARGO_TARGET_DIR` 或暫存快取，不全域改 PATH，不碰 Runtime Host。另修正 Hook 通知候選：應先套用實際 Provider 啟用條件，才限制最多 128 個有效 Session，避免被停用 Agent 擠掉有效通知 | `disabled_provider_flood_cannot_starve_late_enabled_host_agent` **RED：實際 0，預期 1**；CLI 舊建置的 C compiler 環境無法自動找到 MSVC | **GREEN：GPUI Rust Library 416/416 PASS**、Native `cargo check --offline --locked --bin alera-gpui-desktop` **PASS**；採用該腳本的 Windows CLI `cargo check --offline --locked -p alera-cli --bin alera -j 2` **exit 0（8m41s）**；兩項 CLI H6 測試的測試版原生依賴仍在編譯，**未宣告 PASS** | **僅獨立 Worktree**；Windows 實機通知、Classic/GPUI 所有權仍未驗收 | 完成 CLI H6 專項、`git diff --check`、提交功能分支；仍須 Host CAS 與多客戶端通知所有權 |

| 2026-11-11 H6-4 CLI 型別／執行驗證分離 | H6-3 的 MSVC 腳本新增 `check-tests` 子命令，讓 Windows CLI 測試程式可以先做 metadata/typecheck，而非必須連結整支大型測試 EXE；建置全程仍使用 `--offline --locked`、限制 `-j 2`、子程序 `vcvars64.bat`，不碰正式 Host | CLI `cargo test --bin alera agent_hook_readback` 已使用修正後 MSVC 環境實際建置超過 **15 分鐘**，仍卡在 `alera-cli` 大型測試版編譯，依本輪上限只終止該測試工作；改跑 `cargo check --tests` 超過 **5 分鐘**，亦在 `alera-cli` 測試程式編譯期間依本輪上限終止，**兩者都沒有取得斷言結果／exit 0** | **CLI 正式 `cargo check` exit 0（8m41s）**；**GPUI Library 416/416 PASS、Native `cargo check` exit 0**。CLI 專項與 CLI test-metadata **NOT VERIFIED**，不得寫成 GREEN；可在資源充足 CI 或較長的本機建置視窗使用 `scripts/verify-agent-hook-cli-msvc.cmd check-tests` 及 `test` 補驗 | 終止範圍只有本輪兩支 CLI 驗證程序；**無合併、Push、Release、Host 重啟、PTY 改動**，`supported=false` | H6-5：完成 CLI 實際專測、設定 CAS；H5 實機 Toast／雙版本仲裁 |

| 2026-11-11 H6-5 原生 Hook CAS／設定 UI | 復用 Host 既有 `runtimeSettings.update` 交易式 `expectedRevision`；新增 `alera settings --no-start --json agent-hooks-cas --agent-id codex --expected-revision <N> --expected-enabled false --enabled true`。CLI 只接受指定 Provider 與預期原值/版本，先由已認證的既有 Host 取得版本快照，拒絕 stale、未知 revision、非法 Provider、非布林舊值。僅送出 `expectedRevision` + `agentStatusHooks`，Host 在 `BEGIN IMMEDIATE` 原子交易中再次驗證 revision；結果只投影 `kind/revision/agentStatusHooks`，不輸出 AI Assist/Token/Prompt，確認版本加一、所有其他 Agent 開關原樣不變。GPUI Agents 設定頁將 Codex／Claude Hook 控制連到 Host CAS，離線、舊版 CLI 或寫入期間不能操作，無樂觀寫入；成功/失敗都重新讀取 Host 並重設通知候選 | CLI 已先寫 `agent_hook_cas_only_mutates_one_switch_and_requires_exact_host_revision`、`agent_hook_cas_cli_requires_observed_value_and_revision`，**測試版執行仍未驗證**；GPUI 新增 `agent_hook_cas_readback_cannot_confirm_stale_or_other_provider_changes` 及版控快照解析斷言，完整 Library **417/417 PASS** | **Native GUI `cargo check --offline --locked --bin alera-gpui-desktop -j 2` exit 0；CLI 使用 VS MSVC script `cargo check --offline --locked -p alera-cli --bin alera -j 2` exit 0**。Windows GUI 真實點擊、隔離 Host 設定修改／CAS 衝突、Classic 同時更新及跨客戶端通知仲裁**未驗收**，不可宣稱 H6 整體完成 | **僅獨立 Worktree；未對正式 Runtime Host 執行任何 CAS；`supported=false`** | 下一輪跑 Windows 可完成的 CLI `#[cfg(test)]` 驗證與隔離 Host/GPUI 真實 CAS，逐 Agent Hook 列表擴充，H5 Toast/AppID 單一 owner |

| 2026-11-12 H6-6 獨立 CLI CAS 合約測試與 CI | 將 CLI 現有的 `native_agent_hooks_projection`、`native_agent_hook_cas_payload` 與 CAS Host 回應驗證抽出至**同一份正式 CLI 原始碼** `rust/alera-cli/src/agent_hook_cas_contract.rs`，不是複製測試邏輯。另建立獨立小型 Cargo package `scripts/agent-hook-cas-contract-tests`，在舊 Windows 巨型 CLI 測試 EXE 難以連結時，直接編譯該原始碼測試純邏輯；新增極限版本防護：`i64::MAX - 1` 不得寫到 Native 無法讀回的 `MAX`。GPUI 既有跨平台 model-tests 矩陣增加這一步及對應 PR 路徑觸發（不新增 runner） | **RED：** 先建立測試入口，`cargo test --offline` 因找不到正式 CAS 模組而退出 101。**GREEN：** 正確抽取同一份 CLI 模組後，8 項實測全部 PASS（預期版本、Classic 已更動版本／同 Provider 原值、其他 Provider 不變、秘密欄位過濾、錯誤 Host 回應、64 筆上限、no-op、版本耗盡） | **`cargo test --offline --locked -j 2`：8/8 PASS，exit 0；正式 CLI `cargo check`（process-local VS/MSVC，offline+locked）：exit 0。** Rustfmt、`git diff --check` PASS。GitHub CI workflow 已接線，但**CI 遠端實際執行 NOT RUN** | **沒有啟動／修改正式 Host；此套測試只驗證 CLI 純合約，不等同 Host SQLite 交易競態或 Windows GUI 已驗收；Clap CLI 子命令解析測試仍在大型原生 CLI 測試版待執行。** `supported=false` | 隔離 Host 真正 `BEGIN IMMEDIATE` CAS 競態 E2E、Classic 雙開／Native UI Toggle 真點擊、H5 Toast owner 仲裁 |
| 2026-11-12 H6-7 隔離 Host 雙客戶端 Hook CAS 驗證 | 使用 `actor_test_harness::test_actor` 的兩個已認證本地客戶端、暫存 SQLite 與正式 `runtimeSettings.get/update` 路由，新增 `agent_hook_host_cas_rejects_stale_classic_native_writers_and_preserves_others`。驗證 Classic 更新無關設定也會使 Native 舊版本失效、Native 改 Codex 後 Classic 舊版本不能覆寫、最新版本可改 Claude 且保留 Codex/其他設定；新案例加入 GPUI 實驗 Windows CI 的既有 CLI 測試 Job | 正式 CLI CAS 共用原始碼的獨立合約測試本輪重跑 **8/8 PASS**；CLI 抽取新模組後主程式 `cargo check` **exit 0**。Host 雙客戶端新測試的 `cargo check --tests` 正在驗證，**CLI 大型測試執行檔／CI Host 斷言暫未宣告 PASS** | 僅隔離 actor／暫存資料庫，無正式 Host 寫入、無 Windows GUI／Toast；`supported=false` | 完成新 Host Test 編譯與實際斷言，繼續 Classic/GPUI 雙開及 Windows 原生切換驗收 |
| 2026-11-12 H6-7b 編譯驗證補記 | 正式 CLI `cargo check`（MSVC 子程序環境）**exit 0**；包含新雙客戶端 Host 測試的 `cargo check --tests --bin alera` **exit 0**；共用正式 CLI CAS 原始碼的小型合約套件 `cargo test --offline --locked` **8/8 PASS**；`rustfmt --check`／`cargo fmt --check`／`git diff --check` **PASS** | Host `ServerActor` + SQLite 雙客戶端案例本輪僅通過**編譯檢查**，大型 CLI 測試執行檔與 GitHub CI 尚未實際執行，**斷言 NOT VERIFIED** | 無正式 Host 或 Native GUI 變動；`supported=false` | 在 CI 實際執行新 Host 測試並檢查 Classic/GPUI 視窗行為 |

## 尚未解決

- 主 GPUI 整合 Worktree 已有 unrelated WIP/test fixture；本功能不碰它們。
- 既有 Classic Host 版本可能不支援新版部分共用設定；不能據此升級正式 Host。
- 最新 Windows GPUI EXE 曾被工作區外可執行檔安全政策禁止啟動；須使用授權隔離途徑驗收 GUI，不得搬檔繞過。
- 跨平台 OS 通知需分平台 adapter；Windows 應先通過，其他平台標示 NOT RUN 而非 PASS。
- H4 已比對 Host `stateStartedAt` 支援同狀態的新 Agent 回合通知；H5 通知決策已接到 Native Host 事件／清單及 Windows API，但 release 通知投遞權尚未正式交接。**Library 413/413 PASS 不等於系統 Toast／跨版本仲裁已通過**。
- 主程式仍保留既有 `terminal list --no-start` 每 3 秒（有活躍 Agent）或每 15 秒（沒有）刷新作為 **Host push 的回退方案**；真實 Host 事件／斷線重連尚待隔離端到端驗收。
- H3-2 已加入同一 Native Terminal stream 的 Host `agentPresenceChanged` push 轉接，CLI/Native 單元測試均 GREEN；但當沒有開啟任何 Native Host 預覽 Tab、stream 尚未啟動時仍只依賴 3s/15s 的授權清單輪詢；完整所有 Worktree 全域訂閱、隔離 Host 真實 Hook 與重連驗收仍屬後續工作。
- H5 Toast 已可編譯並接入 Native event loop，但**目前只有 Windows Debug 且明確設定 `ALERA_GPUI_AGENT_TOAST_EXCLUSIVE_TEST=1` 才可能發送**；這是開發者人工隔離測試開關，不代表完成跨版本 OS 通知仲裁。正式版、未開啟的使用者一律不接管 Classic 通知，以免重複；Host settings 若缺少通知偏好／Host 失聯，亦一律不投遞。
- H5 Toast 選取僅自動定位到**當前已經驗證的 Host Workspace**，不能將跨工作區通知當成已完成。真實 Windows AppID shortcut、前景視窗啟動與 Toast click 的獨立實機 Gate 尚未通過；OS 不支援或通知顯示失敗會回報錯誤而非假成功。

## 發行 Gate

- 所有 H1–H6 完成且 H7 真實驗收成功前，`supported=false`。
- 每次任務交接請先讀本檔進度，再看 Git Commit；若 RED／GREEN 部分未完成，保留 TODO，不能假完成。
