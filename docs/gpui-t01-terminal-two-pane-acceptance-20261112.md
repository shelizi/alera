# Alera GPUI T01：主、副 Pane New Terminal Windows 正向驗收

日期：2026-11-12（Asia/Taipei）。結論：**完成（正常流程）／PASS**。本項是功能驗收，**不代表** `supported=true` 或全面可靠性門檻已解除。

## 測試邊界

- 分支：`integrate/gpui-central-diff-into-main-20261112`，測試開始 HEAD `165c79824`；使用該整合工作樹既有已編譯的 `desktop_gpui/target/debug/alera-gpui-desktop.exe` 與相容的 `alera-cli.exe`。本次無 Rust 程式碼變更、無重編譯。
- 工作目錄：既有、受保護的巢狀測試 Git fixture `.gpui-center-diff-smoke`，只預覽一個檔案並執行無副作用 `echo`，不建立／刪除 Git worktree。
- CLI Host Runtime：僅 `desktop_gpui/target/smoke-terminal-t01-runtime-20261112-1900`；GPUI State：僅 `desktop_gpui/target/smoke-terminal-t01-state-20261112-1900`。用 `ALERA_RUNTIME_DIR`、`ALERA_GPUI_STATE_DIR`、`ALERA_CLI_PATH` 明確隔離，未連到正式 Classic Runtime。
- 在隔離 Runtime 用 CLI 登錄既有 fixture project；GUI 啟動前 `runtime status` 為 `running=false`。只有實際點擊 `+ → New Terminal` 才啟動 Host。

## 真實 Windows GUI 正向操作

| 驗收點 | Windows GPUI 畫面 | Host 獨立讀回 |
| --- | --- | --- |
| 主 Pane `+ → New Terminal` | 建立真實 `cmd.exe` 分頁，顯示 fixture 命令提示字元 | 第一個 `running=true` Host-owned PTY |
| 主 Pane 輸入／Enter | `echo gpuit01primaryok` 顯示 `gpuit01primaryok` 並返回 prompt | 第一 Session `terminal read` 含主 marker |
| 開啟副 Pane | 使用 Pane Actions 的 `Move tab to second pane` 建立混合雙 Pane；原 Session 仍保留 | 第一 Session 仍 `running=true` |
| 副 Pane `+ → New Terminal` | 從副 Pane 自己的 `+` 建立另一個 `cmd.exe` | `terminal list` 有兩個不同 handle／tabId 的 `running=true` Session |
| 副 Pane 輸入／Enter | `echo gpuit01secondaryok` 顯示 `gpuit01secondaryok` 並返回 prompt | 第二 Session `terminal read` 含副 marker；兩份輸出沒有交叉 |

已核對的 Host 身分：

| 建立來源 | Session Handle | Host Tab ID |
| --- | --- | --- |
| 主 Pane | `1003909c-7e97-44c9-b87b-3ab6276468db` | `94402f02-04fc-4a57-af98-713879e6d649` |
| 副 Pane | `e0d91467-7b95-436c-a86b-30a5ed04cd1a` | `fc3bf6c5-ba7d-4ac9-83b4-80457c68058f` |

兩個 Session 同屬註冊的**隔離測試 Workspace** `10120f56-03e4-4a0d-a32f-bd2fb77aaae7`。兩份 Host 輸出交叉檢查：主 `primary=true, secondary=false`；副 `primary=false, secondary=true`。並未以相同 Session 冒充新分頁，也沒有錯誤回報建立成功。

## 收尾及剩餘界線

- 正常關閉本次 GPUI GUI（`Alt+F4`）；只對指定隔離 Runtime 執行 `runtime stop --force` 收回兩個測試 PTY。再次讀取隔離 `runtime status` 得到 `running=false`、`host=null`，沒有操作正式 Classic Host、`main`。
- 測試 fixture 在本次前後 `git status --short` 均為 `M center-diff-smoke.txt` 和 `?? center-diff-source-switch-untracked.txt`；既有測試修改和未追蹤資料未被覆寫、清理或提交。整合分支的 `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/` 仍保持原有未追蹤狀態。
- 測試桌面的注音中文模式會在輸入英文鍵時進入選字；切到英文模式後普通 Shell 指令正常。繁中 composition／候選窗／複製貼上留給 **T06**，本次不擴大。
- 本輪新增的證據是實際 Windows GUI／隔離 Host 正向流程。沒有新程式碼缺口，故不新增不必要程式修改，也未重跑先前已通過的完整 Cargo suites。
- **下一項 NOW：T02**（已安裝 Agent 偵測、刷新、未安裝提示與 Classic 菜單對齊）。`supported=false` 保留；無 `main` 合併、Push、部署或發版。
