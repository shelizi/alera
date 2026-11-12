# Alera GPUI T04：雙 Pane Host Session Attach／Tab 切換／關閉後重新附加

日期：2026-11-12（Asia/Taipei）。分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD：`30d5db67a`。**T04 一般 Windows GUI 流程 PASS**，但重啟後左右 Pane 分配未保留，已明確移到 **T05** 正常功能缺口；此項不代表 `supported=true`。

## 測試邊界

- 只用 T02 已建置的 Windows GPUI／`alera-cli.exe`，不修改 Rust／Classic 程式碼或本機 Agent 安裝。
- 既有、不可清理的 `.gpui-center-diff-smoke` 巢狀測試 Git 目錄只作目前 Workspace。獨立 Runtime：`desktop_gpui/target/smoke-terminal-t04-runtime-20261112`，獨立 GPUI State：`desktop_gpui/target/smoke-terminal-t04-state-20261112`；明確設定 `ALERA_RUNTIME_DIR`、`ALERA_GPUI_STATE_DIR`、`ALERA_CLI_PATH`。本輪先以 CLI 登錄已存在的 fixture Project，後續所有建立／Attach 都由 GPUI 畫面完成。
- 所有輸入僅是 `cmd.exe` 的無副作用 `echo`，不寫檔、不操作真實 Agent。每個步驟用隔離 Host 的 `terminal --no-start list/read` 驗證身分和輸出。

## Host 身分（整個驗收中皆不變）

| Session | Session Handle | Host Tab ID | Workspace ID |
| --- | --- | --- | --- |
| A（初次由主 Pane 建立） | `e23fb7fa-de52-4db8-a8be-068869f7f3f2` | `23c0f38b-2c90-41dd-be09-73820766cdec` | `7ae172db-43df-469d-92fd-f011c8f70472` |
| B（第二個普通 Terminal） | `d2728728-ea16-4ed0-9c04-13262955d9d2` | `ce5db355-525a-43b3-a498-b7a06a21ad92` | `7ae172db-43df-469d-92fd-f011c8f70472` |

## Windows GUI／Host 雙向正向驗收

| 步驟 | GUI 操作及觀察 | Host 驗證 |
| --- | --- | --- |
| 建立、切換 | 主 Pane 自己的 `+ → New Terminal` 建立 A／B 兩個 `cmd.exe`；逐一輸入 `echo t04primarybefore`、`echo t04secondbefore`，切換 Tab 讀到各自歷史 | `list` 有 A、B 兩個 `running=true`；每個 Session `read` 只有自己的 marker |
| 關閉預覽、不停 Host | 在 GPUI 按 A Tab 的 `x`，B Tab 保留；畫面中的 A Tab 移除 | A、B **仍 `running=true`、原 Handle／Tab ID 不變** |
| 主 Pane 重新 Attach | 從主 Pane `+ → Attach running Host session` 選原 A，看到原先輸出，再從此 GUI 發出 `echo t04primaryafter` | A 的 `read` 有前後 marker；B 不含 A marker，數量仍是兩個 |
| 副 Pane 正常配置 | 從既有 Tab 右鍵 `Move to second pane` 將 A 移到副 Pane，保留 B 在主 Pane；副 Pane使用 `+ → New File` 新增**未在測試 Git fixture 建立檔案**的暫時 Editor Tab，以免關閉唯一一個 Tab 時 Pane 自動折疊 | 兩個 Host Session 都未改變 |
| 副 Pane 關閉及重新 Attach | 關閉副 Pane 的 A Terminal Tab，從副 Pane 自己的 `+ → Attach running Host session` 選原 A；原歷史完整；再執行 `echo t04rightreattach` | A 的 `read` 確認右 Pane 新 marker、A 原始 Handle 不變；B 仍 running |
| 兩 Pane 互不串流 | 切到左 Pane B，以一般英文鍵盤執行 `echo t04leftretry`；副 Pane A 保留 `t04rightreattach` | B `read` 只含 B marker，A `read` 只含 A marker |
| GUI 正常關閉再開啟 | `Alt+F4` 關閉 GPUI（不停止 Host），以**相同隔離 Runtime／State** 重開；兩個既有 Terminal Tab 與文字歷史都恢復，切換不同 Tab 可以繼續輸入 `echo t04restarttab2`（A）和 `echo t04restoredleft`（B） | 重開前後 `list` 都是**相同兩組 Handle／Tab ID**，兩個仍 running；`read` 各自只有對應新增 marker，無新建第三個 PTY |

## 已確認的正常功能缺口（T05）

**重開 GPUI 後，Host 自動恢復了兩個 Session，但之前的主／副 Pane 分配沒有恢復；畫面退回單一主 Pane 的兩個 Terminal Tab。** 這是功能完整度缺口，不是 Session 遺失或輸出串線。

程式檢查：`SavedWorkbenchLayoutV1` 目前保存 `split_enabled`、軸及 Editor keys；`apply_restored_mixed_panes` 註解明確說 **Host IDs never restore**。目前的 TerminalLayout 儲存兩個 Host 身分和 active handle，卻沒有分別保存 Host 在哪個 Workbench Pane。下一項 **T05** 要規劃/實作有 Host 身分校驗的 Pane 位置還原（需要完整 Session/Workspace/Tab triple，僅在仍存活的 Host Session 上還原；不可自動開新 PTY）及 TDD，然後用正常 GUI 重新啟動驗收。

本輪切換左右 Pane 後有一次在快速連續點擊與貼字的測試工具操作中未看到新輸出，重新聚焦並發出 `echo t04leftretry` 即正常；無法由這次單一操作證明是產品輸入遺失。將輸入法／切換焦點後首鍵的普通使用情境留 **T06**，不誤判為已確定的競態 bug。

## 收尾

- 第二次 GUI 正常關閉後，只對 `smoke-terminal-t04-runtime-20261112` 明確執行 `runtime stop --force`；回應 `stopped=true`、`activeSessions=2`，最後隔離 `runtime status` 回報 `running=false`、`host=null`。沒有接觸正式 Classic Host。
- 巢狀 fixture 驗收前後 `git status --short` 都是原有 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt`；沒有刪除、重設、提交，原未追蹤的 `.gpui-center-diff-smoke/`／`desktop_gpui/target/` 均保留。
- 本次只是 Windows 正向功能驗收，沒有產品程式碼變更，未重跑完整 Cargo；T02 的 513/513 Library PASS 是歷史結果而非本輪。
- **T04 主／副 Pane Attach、Close、重新 Attach、Session ID、不終止背景 PTY、GUI 重開後的兩 Session 恢復皆 PASS → NOW T05**。其中原 Pane 位置恢復 **FAIL／T05 待修**；`supported=false`，無 `main` 合併、Push、部署、發版。
