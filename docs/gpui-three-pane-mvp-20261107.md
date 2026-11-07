# Alera GPUI 三欄核心工作台（2026-11-07）

## 本階段範圍

僅做可用的原生桌面 MVP，不再擴充完整 IDE parity；此分支仍為 **experimental / unsupported**，不替換正式 Flutter 桌面版。

| 位置 | 功能 | 實作及限制 |
| --- | --- | --- |
| 左側 Projects | 目前 Workspace、最近 12 筆專案、原生資料夾選取／切換；Runtime Host 狀態、既有 session 清單 | 切換保留每個 Workspace 的本地 preview/draft 狀態，未儲存編輯會阻止切換；不是完整資料庫專案管理器 |
| 中央 Terminal（預設） | 多個 Host session 預覽頁籤、ANSI VT 畫面／Text log、定時更新、輸入欄、Send 與 Run（送 Enter） | 透過既有 Alera CLI `terminal --no-start` 的 list/read/write；Host 保留 PTY / Shell 所有權；只向使用者明確選取且仍在執行的 session 送資料 |
| 中央 Editor（切換） | 從檔案總管與搜尋結果開啟 UTF-8 檔案、基本編輯、Undo/Redo、Save／Save As、分頁、未命名草稿復原 | 編輯器先保持基本功能，未擴充 LSP、完成提示或複雜 IDE 面板 |
| 右側 Explorer + Git | 檔案樹／一般瀏覽、Quick Open、內容搜尋、Git 變更／唯讀 patch | 檔案路徑使用既有 canonical / workspace 安全檢查；Git writer、merge、reset 暫不開放 |

三欄透過 GPUI `flex_row_reverse` 排列，內部結構仍為 Explorer、中央 Editor/Terminal、Projects；視覺順序則為 Projects / Terminal(or Editor) / Explorer+Git。

## Terminal 控制與保護

- 不起第二套 PTY、不直接讀寫 token、不直接連 Host RPC、不由 GPUI 自動啟動 Runtime Host。
- 選取 Host session 後才顯示輸出與原生命令輸入欄；資料由 `terminal write --handle ... --stdin [--enter]` 的子行程標準輸入管道送入，避免命令內容暴露在子行程命令列。CLI `--no-start` 保持強制。
- 支援貼上、IME 組字暫緩送出、Enter（只在 Terminal 命令輸入欄有效）、Send 與 Run 按鈕；一次最多 512 bytes 原生輸入（CLI bridge 上限 4 KiB）。
- 不讓過期非同步回應更新其他 Workspace／已切換的 session；輸入成功後只有在文字未被修改時才清空欄位。
- `Host sessions` 列表是**所有 Workspace**，目前沒有完整的 Workspace 對應過濾或 Host attach/resize lease。CLI 不支援時採 fail-closed 錯誤提示。
- **未完成**：在 GPUI 建立新的 Host Terminal session、視窗隨尺寸 PTY resize、Alt/Ctrl/方向鍵 TUI 完整鍵盤轉送、滑鼠 TUI、複製選取、Microsoft Zhuyin GUI 實測。

## 執行與驗證

```powershell
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline
cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline
cargo fmt --manifest-path desktop_gpui/Cargo.toml --all -- --check
cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline
```

三欄及 Host 輸入橋接實作後：**110/110 model tests PASS，Windows native cargo check PASS，Windows cargo build PASS，cargo fmt --check PASS，git diff --check PASS**。執行隔離 GUI 啟動時，程式曾維持執行，但無法取得可確認的 GPUI 視窗畫面，故 **真實 GUI 三欄排版、注音與 Host 輸入端對端驗收仍未通過／待測**，不可把編譯結果等同實機驗收。

### 下一次最小人工 GUI 驗收

1. 以隔離測試資料夾及 `ALERA_GPUI_STATE_DIR` 啟動 GPUI；確認右側有 Explorer + Git、左側有 Projects、中央預設 Terminal。
2. 點擊左側不同 Workspace，確認專案清單與右側 Git／Explorer 同步變化，Terminal 不保留前一 Workspace 的本地 session 選取狀態。
3. 連上隔離 Runtime Host session，測試中文輸入欄、Send、Run/Enter 與收到的 VT 輸出；重送、停用 session 及 CLI 離線時不得無限迴圈或改動其他 session。
4. 右側開啟文字檔進入 Editor，做 Unicode 與 CRLF 修改、Undo/Redo、Save/Save As、關閉與重新開啟草稿；核對寫入內容與檔案外部衝突拒絕。
5. 實機檢查 Microsoft Zhuyin 選字窗、Windows 多 DPI、長檔 601／5000 行、視窗 resize；未通過前仍不允許標示 supported。

## 發布界線

只允許在 GPUI 隔離工作樹與其開發分支驗證。此階段不變更正式 `main`、Flutter 桌面、Runtime Host 生命周期，不推送、不打 tag、不 Release。
