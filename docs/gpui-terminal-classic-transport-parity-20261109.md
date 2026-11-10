# Alera Native GPUI：沿用 Flutter Classic Terminal 架構

**決策：改用 Classic 已驗證的 Runtime Host 長連線、Session 事件與輸出節流語意；不要把 CLI 每鍵啟動／定時讀取當成正式 Terminal 傳輸。**

範圍：`integrate/gpui-p0p1p2-20261108` 的設計與遷移規劃。Native 仍是 experimental／`supported=false`。本文件不核准推送、合併 main、發布或中斷任何現有 Host/PTY。

## 原始碼比對

| 面向 | Flutter Classic 已有做法 | GPUI 現況 | GPUI 目標 |
| --- | --- | --- | --- |
| Host | 同一套 Rust Runtime Host 擁有 PTY、生命週期、工作區與權限 | 同一套 Host（正確） | 完全共用，不複製 PTY |
| 傳輸 | `SocketTerminalHostClient` 持續認證 Socket 連線 | 每筆 `terminal read/write` 啟動 CLI | 一個受控、可重連的長連線傳輸 |
| 輸出 | Host `output` 事件、二進位輸出框架、失步重新同步 | UI 100/350/1500ms 輪詢，最多 64KiB/次 | Host 推播，無固定遠端 read 輪詢 |
| Session | `eventsForSession`，每個 session O(1) 分派 | `TerminalDock` 的清單＋cursor 比對 | Host 身分驗證後訂閱／取消訂閱每個 session |
| 繪製 | `TerminalSessionOutputPump` 與輸出待處理佇列：vsync、FPS 節流、4–64KiB 自適應批次、背景 catch-up | 收到 chunk 後更新 `VtScreen` 與 GPUI View | 重用*策略*，以 Rust/GPUI 實作繪製調度，避免 Flutter Widget 依賴 |
| 輸入 | 持續連線的 `write`，輸入優先刷新下一 frame | 每批按鍵透過短命 CLI | 直接經過同一條已認證傳輸排隊寫入，無每鍵 spawn |
| 復原 | 恢復 cursor、output resync、pause/resume、連線錯誤恢復 | 輪詢 cursor／有限 Host 身分檢查 | 保留三重身分（workspace/tab/session）與 view lease，實作安全重連／snapshot |

核心參考：

- `lib/src/features/workbench/infra/terminal_host/terminal_host_client.dart`
- `lib/src/features/workbench/infra/terminal_host/terminal_host_client_connections.dart`
- `lib/src/features/workbench/infra/terminal_host/terminal_host_client_session_events.dart`
- `lib/src/features/workbench/infra/terminal_host/terminal_host_pty_session.dart`
- `lib/src/features/workbench/presentation/terminal_runtime_output_pump.dart`
- `lib/src/features/workbench/presentation/terminal_runtime_output_batching.dart`
- `rust/alera-cli/src/runtime_host_client.rs`
- `rust/alera-cli/src/terminal_host/server/output_delivery.rs`
- `desktop_gpui/src/runtime_bridge.rs`, `terminal.rs`, `main.rs`

## 建議最小切換方案

1. **保留現有 Host 協定及所有現有 Session**。以 CLI 所擁有的認證能力做一個*常駐、窄權限* Native Terminal transport adapter（可沿用／抽出 `runtime_host_client`），不讓 GPUI Widget 讀取控制檔 token。若抽成共用 Rust crate，也要保護 token 並讓 GUI 只能拿到窄 Terminal capability。
2. Adapter 啟動時嚴格 `--no-start`，與**已存在**的 Host 握手；協調 `createOrAttach`／`viewLeaseId`／`resumeCursor`，不以重新訂閱為由建立 shell，不偷走 Classic 的 driver lease，也不把 Host 關閉當成 UI Tab 關閉。
3. 保留 Host 二進位 output、`outputResyncRequired`、exit/error/driver/resize 事件語意。以 bounded channel 將事件按 Session 分派到 GPUI；必須處理 UTF-8 chunk 邊界、順序、斷線、cursor gap、保留輸出與落後時 snapshot。Host 端已有背壓與 resume 機制，不能繞過。
4. GPUI 每個 Terminal Surface 只負責鍵盤／IME、VT 狀態、畫面與焦點。Rust 應用層 service 維護 transport、連線、重試、session registry 與 output queue；不要讓 `main.rs` 的 Render 方法持有 socket 或阻塞 RPC。依專案 `alera-di-architecture` 技術規範，使用窄能力 port 與 constructor injection，避免隱藏全域 service。
5. 參考 Classic 的 per-session output pump：小段輸出優先下一 frame、持續 TUI 輸出有 FPS 上限、一次至多消耗自適應預算、大量資料保持順序且不在 UI thread 全量解析；背景關閉繪製、返回時安全 catch-up。
6. 完成端到端驗證後才停用 `RuntimeCliBridge::read_terminal/write_terminal` 常規路徑。原本 CLI 路徑保留為明確故障恢復／功能偵測 fallback，不應讓正常狀態一直短命 spawn。先不要移除之前的 64KiB／100ms 過渡優化，以免新增協定尚未驗收就退化。

## 驗收條件（不能靠編譯通過取代）

- 在隔離 Host 工作區新建 Terminal、打字立即回顯、Enter／Backspace／Ctrl／方向鍵、繁體中文注音 IME composition／commit、剪貼簿。
- `cmd`、PowerShell、長時間 TUI、ANSI 顏色與游標移動、頻繁 resize、雙分割 pane、Tab 切換、捲動、長輸出不中斷／不亂序。
- 客戶端落後或離線重新連接時不遺失／重複輸出；兩個 Native/Classic 視窗同時存取不搶 session、不得關閉現有 PTY。
- 量測 50th/95th percentile **按鍵到畫面回顯延遲**、持續輸出 throughput、CPU／memory、UI frame time，與 Classic 同機比較，不能只量 CLI round trip。
- 先 Host 隔離測試與 Rust focused tests，再 Windows GPUI 實機，最後 macOS；`supported=false` 直到門檻完成。

## 風險與邊界

- Classic Dart `SocketTerminalHostClient`／xterm Widget 不應直接塞進 GPUI；重用的是*協定、Runtime Host、事件語意、輸出調度策略*，不是把 Flutter Engine 藏在 Native 中。
- 目前 `rust/alera-cli/src/runtime_host_client.rs` 的 CLI 命令型客戶端會忽略普通 Host push events，不能僅將它常駐就宣稱完整 Classic parity；event/binary frame 接收、恢復、flow control 需要明確實作與測試。
- GPUI 過渡版 100ms polling 是改善 1.5s UI 延遲的已提交 workaround，**不是**這次確立的目標架構。這份文件只完成實際源碼比較與決策，尚未將 Native 切到推播。
