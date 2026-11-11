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
| H2 | GPUI 版面渲染 | Tab 與左欄顏色、Tooltip、等待／阻塞、重繪後復原 | 四態顯示／無重疊、其他 Tab 不變 | **PARTIAL：Tab 投影與色彩 GREEN；左欄/Tooltip/GUI 尚未驗收** | Library **394/394 PASS**，Native `cargo check --bin alera-gpui-desktop` PASS |
| H3 | Host Presence 同步 | 首次快照、delta、亂序、重連、失聯、取消、切換 Workspace／世代 | 唯一 Host 真相、增量正確、過期請求拒絕 | TODO | — |
| H4 | 通知規則 | 60s cooldown、3s/10s coalesce、done toggle、初始化／重連不補發、關閉 Session 去重 | 純 Rust 可重現與無資料競態 | TODO | — |
| H5 | Windows OS 通知／點擊 | 許可、關閉通知、同時兩版去重、Windows AppId 不污染 Classic、點通知定位正確 Pane/Tab | 具體隔離桌面通知可見且導航安全 | TODO | — |
| H6 | 設定與跨版本 | Host 設定讀寫 CAS、停用、舊 Host、離線錯誤與重試 | 設定真實生效、Classic / GPUI 行為一致 | TODO | — |
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

## 尚未解決

- 主 GPUI 整合 Worktree 已有 unrelated WIP/test fixture；本功能不碰它們。
- 既有 Classic Host 版本可能不支援新版部分共用設定；不能據此升級正式 Host。
- 最新 Windows GPUI EXE 曾被工作區外可執行檔安全政策禁止啟動；須使用授權隔離途徑驗收 GUI，不得搬檔繞過。
- 跨平台 OS 通知需分平台 adapter；Windows 應先通過，其他平台標示 NOT RUN 而非 PASS。

## 發行 Gate

- 所有 H1–H6 完成且 H7 真實驗收成功前，`supported=false`。
- 每次任務交接請先讀本檔進度，再看 Git Commit；若 RED／GREEN 部分未完成，保留 TODO，不能假完成。
