# Alera Flutter Classic → Native GPUI：元件重用、拆層與 TDD 計畫

日期：2026-11-09。工作分支：`integrate/gpui-p0p1p2-20261108`。

目標不是讓 GPUI 載入 Dart Widget，而是保留 **Classic 已驗證的 Host、協定、互動契約與業務規則**，將非視覺邏輯隔離，再由 GPUI Kit／Rust Native UI 消費。Classic/main、既有 Host PTY、使用者未儲存內容須保持不變。Native `supported=false` 直到實機驗收。

## 元件盤點與重用策略

| 優先 | Flutter/Dart 來源 | 核心重用／優化目標 | Native 接合點及 TDD 契約 |
| --- | --- | --- | --- |
| P0 | `terminal_host_client.dart`, `terminal_host_client_session_events.dart` | Session O(1) fan-out、global observer、釋放後隔離、重連錯誤；將 socket-無關邏輯抽離 | `terminal.rs` 的 Host Session registry；測試跨 Session 不串流、Session ID 重用不交叉、斷線只通知有效訂閱 |
| P0 | `terminal_host_client_connections.dart`, `terminal_host_client_socket_reader.dart`, `terminal_host_frame_codec.dart` | 認證長連線、binary output、協定能力交涉，reader/decoder offload；不複製 token 或 PTY owner | Native 窄權限持久 bridge；先做 mock Host handshake、push、backpressure、resume、斷線重接測試，取代 CLI 每次 spawn 與定時 read |
| P0 | `terminal_runtime_output_pipeline.dart`, `terminal_runtime_output_pump.dart`, `terminal_runtime_output_batching.dart` | 累積輸出、4–64KiB 自適應預算、低延遲 interactive flush、背景節流和可恢復 backlog | `terminal_output_budget.rs` 與 GPUI `VtScreen`；測試快慢 parse、空閒 reset、輸出順序、UTF-8 邊界、TUI 大輸出 frame budget |
| P0 | `terminal_host_pty_session.dart`, `terminal_host_pty_session_lease.dart` | `createOrAttach`、Session identity、`viewLeaseId`、`resumeCursor`、pause/resume、snapshot、detach/terminate 分離 | 保留 `workspaceId/tabId/sessionId` 三重身分；測試跨 Worktree 轉移、過期 callback、斷線、關閉視圖不結束 Host PTY |
| P0 | `terminal_runtime_user_input.dart`, Terminal/IME/Clipboard UI | 輸入意圖與 PTY bytes 分離，Ctrl/Shift/IME commit 僅傳一次；避免每鍵啟 CLI | `terminal.rs::encode_terminal_key`、`main.rs::flush_terminal_key_queue`；測試輸入次序、目標 Session 切換、連續快速輸入與 IME |
| P1 | `workspace_workbench_tab_strip.dart`, `workspace_workbench_tab_strip_drop.dart`, `workbench_tab_placement_plan.dart` | Tab 預覽/固定、關閉、拖曳/分割、撤銷與跨窗格狀態轉移 | `workbench_tabs.rs`, `workbench_panes.rs`, `terminal_panes.rs`；先 pure transition 測試，後 GPUI Kit TabBar |
| P1 | `design_system/layout/alera_dialog.dart`, `alera_confirm_dialog.dart`, `design_system/menus/`, `design_system/forms/` | 保留 modal/focus/escape、選項語意、危險操作確認、選單觸發與 i18n；避免兩份自製組件 | `native_modal.rs`, `native_components.rs`, `native_popup_ui.rs`；優先 GPUI Kit；各種選單／設定從單元規則測試接到 Native |
| P1 | `design_system/feedback/`, `design_system/icons/`, 主題/排版 | 空狀態、通知、狀態燈、file icons、間距／色彩 token | `visual_tokens.rs`, `native_ui_theme.rs`, `native_ui_icons.rs`；視覺快照與高對比／主題回歸 |
| P2 | `workspace_explorer.dart`, `workspace_explorer_reveal.dart`, `quick_open_dialog.dart` | 搜尋排序、預覽/固定、選取、過濾、忽略檔案、跨 Worktree 路徑安全 | `workspace_tree.rs`, `file_search.rs`, `explorer_selection.rs`；資料層測試後對齊樹／Quick Open |
| P2 | `workspace_git_history_*.dart`, `workspace_git_diff_surface_*.dart` | Git Graph ref 導航、Diff 行內/換行、merge conflict、Worktree 快捷切換 | `git_history.rs`, `git_inspect.rs`, `git_actions.rs`；測試安全的唯讀 query 與寫入確認 |
| P2 | `keyboard/application/keybinding_resolver.dart`, `keyboard/domain/` | 快捷鍵衝突、可攔截條件、焦點優先級、平台 key mapping | `main.rs`/Native action routing；test key conflicts、IME 與 terminal focus precedence |

## 已開始的 TDD 批次

1. **Classic Session Event Hub**：新增 `test/unit/terminal_host_event_hub_test.dart`，先定義 Session 隔離／ID 重用／斷線／close 契約；抽出 `lib/src/features/workbench/infra/terminal_host/terminal_host_event_hub.dart`，再把 `SocketTerminalHostClient` 的原 mixin 改為委派，維持對外 `eventsForSession`、`releaseSession`、global event API。
2. **Native 自適應輸出預算**：新增 `desktop_gpui/src/terminal_output_budget.rs` 的 deterministic tests；由 Classic 量測策略移植 pure policy，再接入每個 Native Terminal Tab 的 read/window 計算與 VT parse 回饋。這只能緩解現有 polling 的 frame blocking，**不能**替代 P0 常駐連線。
3. **Transport 下一批**：先測 Host framing／權限拒絕／Session 不變性／一筆鍵盤輸入不啟子行程，再重構可注入的持久 Rust bridge，最後在 GPUI 關閉正常路徑的短命 CLI polling。

## 不可退讓的驗收

- TDD 每批都要有可命名的紅燈案例、實作、綠燈、重構，再做 scoped compile/analyze／`git diff --check`；工具環境阻擋的項目不得聲稱已通過。
- Native 讀取/更新失敗不能讓 Host PTY 消失、不能重啟 Classic Host 或移動現有 session；跨 Worktree 與雙窗格遵守三重身分及 lease。
- UI parity 檢查包含 IME／字母大小寫／Ctrl/Shift、TUI、大量 ANSI、雙窗格、主題、modal/menu、Tab 拖曳與快捷鍵。
- 在 Native 真實 UI 尚未通過鍵盤到畫面的 p50/p95 延遲及輸出 throughput 基準前，**不合併 main、不發布**。

## 2026-11-09 首批執行紀錄

- **Dart（通過）**：從有完整 package cache 的 repo root 執行 integration worktree 的 `test/unit/terminal_host_event_hub_test.dart`，**4/4**。此測試以 pure Dart fake event 驗證 Session 事件 hub，不需要 Flutter UI。
- **Dart（通過）**：`dart desktop_gpui/tests/terminal_event_hub_smoke.dart`，**4 項契約通過**，無 socket／ghostty／Native Assets。
- **Rust RED → GREEN（通過）**：`cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --offline --locked terminal_output_budget`，**4/4**；首輪先因缺少 `TerminalParseBudget` 編譯失敗，實作後全數通過。
- **Rust Native 接線（通過）**：`terminal::tests::native_terminal_read_budget_is_scoped_to_the_active_host_tab`，**1/1**。
- **Rust Host rewind／截斷回歸（RED → GREEN，通過）**：`terminal::tests::native_parse_budget_restarts_after_host_cursor_reset_or_truncation` 首輪預算維持 24KiB、預期 16KiB 而失敗；補上 reset／truncation 時重設預算後，**1/1** 通過。
- **Rust Windows Native 編譯（通過）**：`cargo check --manifest-path desktop_gpui/Cargo.toml --offline --locked`。
- **Dart/Flutter 工作環境限制（尚未通過）**：`flutter test --no-pub` 在 Native Assets 初始化時工具自身報 `Bad state: No element`，未進入測試案例；worktree 直接 `dart pub get --offline` 因 `third_party/xterm` path dependency 缺失而失敗；指定 Dart 檔案的 `dart analyze` 逾時。不可將這些列為已通過。
- **待驗收**：真正的長連線 Host push transport、GPUI/Classic 共同 Host 的 Session lease／snapshot／backpressure、實際 Windows terminal 輸入回顯與大量 TUI 輸出測量。現有工作僅為架構抽層和過渡方案的 VT read 解析預算調節。
