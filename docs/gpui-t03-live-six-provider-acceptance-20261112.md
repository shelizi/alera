# Alera GPUI T03：六個本機 Agent Provider 真實 Windows 正向驗收

日期：2026-11-12（Asia/Taipei）；工作分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD：`edaa07305`。**結論：六個已安裝／已登入 Provider 的正常 GUI 建立、輸入、回覆及 Host PTY 身分核對 6/6 PASS。** 不代表所有錯誤路徑、Hook、跨平台或正式版本已通過；`supported=false` 維持。

## 隔離邊界及做法

- 使用當前分支由 T02 建置的 `desktop_gpui/target/debug/alera-gpui-desktop.exe` 搭配相容的 `alera-cli.exe`。本輪未修改或重編任何 Rust／Flutter 產品程式碼。
- 只以既有 `.gpui-center-diff-smoke` 巢狀 Git fixture 作測試目錄。用全新且獨立的 `desktop_gpui/target/smoke-agent-t03-runtime-20261112`（`ALERA_RUNTIME_DIR`）、`desktop_gpui/target/smoke-agent-t03-state-20261112`（`ALERA_GPUI_STATE_DIR`）、明確的 `ALERA_CLI_PATH` 啟動 GPUI；先在這個 Runtime 註冊現存 fixture Project，未操作正式 Classic Runtime。
- 用 Windows **GPUI 畫面的 `+`** 分別選 Provider，而不是在外部 CLI 假造已存在 Tab；Codex 由主 Pane 建立、Claude Code 由副 Pane 自己的 `+` 建立；其餘四項由主 Pane 新增。主、副 Pane 同時保持獨立的真實 Host TUI。
- 每個 Provider 由 GPUI TUI 鍵盤送出只要求簡短文字回覆、明確禁止工具及修改檔案的訊息。以獨立 `alera-cli.exe terminal --no-start ... list/read` 核對六個不同 running PTY 及輸出；沒有通過畫面就自行宣稱成功。

## 六 Provider Windows 實機結果

| Provider | GUI／實際 CLI | GUI 回覆及 Host `terminal read` | Host Session Handle | 結論 |
| --- | --- | --- | --- | --- |
| Codex | 主 Pane `+ → Codex`；真實 Codex v0.167.0，略過 CLI 更新提示，原有 `YOLO mode` 未更動 | 回覆 `_CODEX_OK`（窄終端的畫面保留此文字，較長測試句前綴未保留）；Host 讀回該回覆 | `57002159-0ef0-4930-9dba-fa0944e1105c` | **PASS** |
| Claude Code | 副 Pane `+ → Claude Code`；真實 Claude Code v2.1.322 | `T3CLAOK` | `23ed8c49-f3d1-4b07-85e8-0a00945f5ec9` | **PASS** |
| GitHub Copilot | 主 Pane `+ → GitHub Copilot`；真實 Copilot v1.0.83 | `T3COPOK` | `3caf74f4-ebe9-4367-9e64-8aec80eaf814` | **PASS** |
| Antigravity | 主 Pane `+ → Antigravity`；真實 Antigravity CLI 1.5.0 | `T3AGYOK` | `7b4bedb3-fff1-4ca2-85e4-95abb8c3c163` | **PASS** |
| Grok Build | 主 Pane `+ → Grok Build`；真實 Grok Build 1.0.77 stable | `T3GROKOK` | `67ca5d03-ba3b-4ba8-84a2-f41dd6da1436` | **PASS** |
| Devin | 主 Pane `+ → Devin`；真實 Devin CLI v3000.12.43 | `T3DEVOK` | `a1d5c3f3-08c8-420e-94e3-f07ce2e6cf19` | **PASS（有非阻塞警告）** |

- `terminal list` 最終同時回傳 **六個不同的 handle/tabId**，全部 `running=true`，Workspace ID 都是 `eea3cf9c-3c06-4a0f-ac73-02374ef36e54`（只屬測試 Runtime）。
- 對每個 Handle 個別 `terminal read --max-bytes=180000`，各自含有其 Provider 對應的單一測試 marker，其他五個 marker 都沒有出現在該 Session；六份輸出皆未截斷。每個 Host Session 都是實際模型回覆而非 `echo` mock。
- 本隔離 Host 未啟用 Agent 狀態 Hook，因此 `terminal list.agentType=null`、`agentState=null` 並不代表啟動失敗；Host PTY 已由畫面及讀回證明正常。Hook 四態與通知另在 H01–H03 驗收。

## 安全與非阻塞觀察

- GitHub Copilot 的信任提示路徑明確是 `.gpui-center-diff-smoke`，只選**本次 Yes**，沒有選擇「remember future sessions」。Antigravity／Grok 的 folder-trust 確認也只針對此隔離 fixture；Provider 是否把信任記錄於個別使用者偏好不在本輪清除或猜測。所有測試提示均禁止工具、修改與執行操作。
- Codex 顯示預先存在的 `permissions: YOLO mode`、更新通知與 Hook 警告；本輪**未啟用**此模式、**未更新** CLI、未設定或核准全域 Hooks。
- **Devin 可正常回答，但顯示 `Hook failed`（指向原有 JBContext 使用者路徑）以及 `Unsupported terminal: Windows Console Host (conhost)` 的提示。** 這是需後續改善／評估 Git Bash 或其他終端相容的正常 UX 缺口，先記錄到 **T06／T07／H02**；沒有更動正式 Hook 或主機環境。
- **未登入／拒絕工作目錄信任**的情況本輪無法直接驗證：六個已安裝 Provider 都有既有有效登入，本次不登出使用者、不破壞憑證。後續 **T07** 須確認無登入、失效憑證及自訂執行路徑錯誤能被明確顯示。這部分**不是** T03 已驗收 PASS 的範圍。
- GPUI 視窗最大化後，部分 TUI 仍只佔 Pane 的一小段寬度；此為 **T06／U01** 的版面／resize 後續觀察，不影響本輪單行訊息真實互動。

## 清理與結果邊界

- 關閉本次 GPUI 視窗（`Alt+F4`），**僅**對明確的測試 Runtime `smoke-agent-t03-runtime-20261112` 執行 `runtime stop --force`，回覆 `stopped=true`、`activeSessions=6`；再查 `runtime status` 確認 `running=false`、`host=null`。沒有停止或改動正式 Classic Host／Session。
- 測試前後巢狀 fixture `git status --short` 相同：`M center-diff-smoke.txt` 與 `?? center-diff-source-switch-untracked.txt`。沒有清理或提交 protected fixture；整合分支原本的 `.gpui-center-diff-smoke/` 及 `desktop_gpui/target/` 仍為未追蹤。
- 本輪屬真實 Windows **功能驗收**，沒有新程式碼變更，所以不重跑 T02 已經通過的 **513/513 GPUI Library**；那是上輪紀錄，並非本輪新測試數字。
- **T03 六種已登入 Provider 正向流程完成 → NOW T04**（Attach、切換、關閉與重接既有 Session）。保留 `supported=false`，不合併 `main`、不 Push、不部署、不 Release。
