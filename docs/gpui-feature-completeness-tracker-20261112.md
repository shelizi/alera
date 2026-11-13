# Alera GPUI 功能完整度優先：Classic 對齊與持續追蹤

> 建立／基線日期：2026-11-12（Asia/Taipei）。
> **本檔是後續 GPUI 功能開發的總追蹤入口與目前優先順序。** 各專題文件保留細節和歷史測試證據，但不再以極端壓力／故障案例決定日常功能的施工順序。
> 工作分支：`integrate/gpui-central-diff-into-main-20261112`；建檔前 HEAD `789f2fa00bacb7709e6562785b05951ed4039da6`。工作樹：`.worktrees/integrate-gpui-central-diff-main-20261112`。
> Classic Flutter 是**互動與功能**的依據；GPUI 外觀不必逐像素相同，但操作流程、結果和重要設定需一致。實驗版仍為 `supported=false`。

## 1. 本階段產品決策（優先遵守）

1. **先讓功能可用、完整，再安排全面穩定性驗收。** 不因少數難以重現的錯誤、極端程序崩潰或競態測試而持續延後一般操作功能。
2. 依序補 **① Terminal／Agent → ② Editor／Explorer → ③ Git → ④ Settings／Rust Core → ⑤ Agent Hook／通知 → ⑥ UI／UX**。單一工作包做好後立即回填本檔，不重做已驗證功能。
3. 每項只要求**正常使用情境的最小閉環**：從 GPUI 觸發 → 實際完成 → 看得到正確結果（必要時檔案／Git／Host 讀回）。新程式碼保留 TDD：先一個可重現的失敗測試，再最小修正、相關回歸與基本 Windows GUI 點擊。
4. **暫緩**：強制關閉 GUI／Host 的極端時序、故意遺失 RPC 回覆、長時間壓力與罕見 PID 競態、多視窗惡意競爭、極大量儲存庫、150%／200% DPI 全矩陣等專門實驗。除非正常操作確實碰到，或之後明確啟動穩定性階段，**不主動開新工作包**。既有防護不可任意移除。
5. 繼續遵守基本安全／正確性：不丟未儲存內容、不誤操作其他 Worktree／PTY、失敗不報假成功、一般 Git 寫入先核對路徑。**不得把安全失敗修成直接略過授權**。
6. 本輪未授權合併正式 `main`、Push、部署、發版或更動 Classic 正式 Host。新功能與驗證先在 GPUI 整合分支與可拋棄測試資料完成；`supported=false` 是**發版狀態**，不是禁止持續實作功能。

## 2. 狀態與證據口徑

| 狀態 | 意義 | 可變更狀態的證據 |
| --- | --- | --- |
| **完成（正常流程）** | GPUI 實際操作、結果讀回及相關回歸通過 | Commit、實測操作、結果與測試 |
| **已有實作／待操作驗收** | Rust 功能和 UI 入口可查，但最新正常 GUI 流程尚未完整確認 | 來源／測試／舊實測；**不可當成完成** |
| **部分完成** | 日常流程某些步驟可用，還存在明確的功能缺口 | 記錄已通過及未接上的步驟 |
| **待補** | 缺少明確功能、UI 接線或操作流程 | 指向 Classic 行為與待做檔案 |
| **待核對** | 文件資訊老舊或實作已變更，尚不能斷言有／無功能 | 先檢查本分支原始碼，再更新 |

**每條工作單固定保留 ID，不以不同分支的歷史 PASS 自動填成目前 GUI PASS。** 此表是 2026-11-12 的初始盤點，不是宣稱 30 項功能全未完成。已核對的實作和過去驗收見末尾來源。行為比對以 Classic 當前原始碼為準。

## 3. 功能追蹤矩陣（依執行順序）

### F1｜Terminal／Agent：先完成新增、操作與雙窗格

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **T01** | 左右 Pane 的 `+` → **New Terminal**，確實建立並可輸入／輸出 | **完成（正常流程）**：2026-11-12 Windows GPUI 主、副 Pane `+` 各建立一個 Host-owned `cmd.exe`；兩個獨立 PTY 都能透過 GUI 輸入 `echo` 並顯示輸出；Host `terminal list/read` 雙向核對 PASS | **PASS**：不同 Session／Tab ID、輸出不串流、無幽靈 Tab。見 [T01 Windows 雙 Pane 驗收](gpui-t01-terminal-two-pane-acceptance-20261112.md) |
| **T02** | `+` 自動偵測本機 Agent；支援刷新、沒有安裝時明示 | **完成（Windows 正常流程）**：新編譯 GPUI 主、副 Pane 都顯示共用 Rust 描述表的六種本機 Agent 與 Classic 名稱順序；隔離 PATH 空清單明示、重新掃描 PASS；TDD + GPUI Library **513/513 PASS** | **PASS**：修正 `New Agent ·` 多餘前綴，保留獨立 Host Profile／Attach；[T02 Windows 驗收](gpui-t02-agent-cli-menu-acceptance-20261112.md)。真實 Provider 互動移至 T03 |
| **T03** | **New Agent** 建立真實 Provider Terminal，登入後可互動 | **完成（Windows 六種已登入 Provider 正向流程）**：Codex／Claude Code／GitHub Copilot／Antigravity／Grok Build／Devin 均從 GUI `+` 建立真實 Host PTY、發送無工具文字問題並收到真實回覆；Host 確認六個獨立 Session，輸出不串流 | **PASS 6/6**：[T03 Windows 六 Provider 驗收](gpui-t03-live-six-provider-acceptance-20261112.md)。未登入／失效憑證的清楚提示尚未驗證，移至 **T07**；Hook 和 Devin conhost 警告移 H02／T06 |
| **T04** | Attach 既有 Host Session；切換／關閉分頁不終止背景 PTY | **完成（Windows 正常流程）**：兩個獨立 `cmd.exe` Host PTY 從 GUI 建立、切換、關閉預覽不停止 PTY，主、副 Pane 的 `+ → Attach` 都能恢復原 Session／輸出並繼續輸入；GUI 關閉重開，原兩個 Session／Tab ID 及輸出仍可用 | **PASS**：[T04 Windows 雙 Pane 驗收](gpui-t04-attach-session-two-pane-acceptance-20261112.md)；**重開後原雙 Pane 位置未保留**，屬 **T05 功能缺口**，不能視為已解 |
| **T05** | Terminal／Agent 分割、Tab 移動、右鍵、拖曳、Close Others／Right | **完成（Windows 正常流程）**：T05A V1 Host 三重身分的雙 Pane 重啟還原已修復，T05B 實際 Claude Code 跨 Pane 拖曳及右鍵移動，修正拖曳後 TUI／TerminalDock 焦點不同步；乾淨 Editor／Terminal 的 Close Others、Close to right、Pin 與未儲存草稿保護均已實機核對，關閉預覽不終止背景 PTY | **PASS**：T05A [雙 Pane 重啟](gpui-t05-host-pane-persistence-tdd-20261112.md)、T05B [真 Agent 與批次關閉](gpui-t05b-agent-tab-and-bulk-close-acceptance-20261112.md)。最終 Library **516/516**、Native Binary **31/31**、Windows 建置 PASS。**NOW T06**；Hook 元資料／失效憑證另列 H／T07 |
| **T06** | Terminal 一般鍵盤、貼上／複製、Enter／Ctrl+鍵、中文輸入 | **完成（Windows 主／副 Pane 真實正常流程 PASS）**：Alacritty Core、滿版／Resize、Ctrl+V／Ctrl+Shift+V 貼上、主副 Ctrl+Shift+C 複製、普通 Ctrl+C、Unicode 已驗收。2026-11-13 新增 native IME open+NATIVE 模式保護第一個注音 ASCII KeyDown，marked 組字期間候選控制鍵不送 Host；真正 Microsoft Bopomofo `S U 3` 主 Pane 候選「你」及副 Pane 選字「擬」均由獨立 Host PTY 回讀且無 `su3` 洩漏。跨 Pane 取消未提交候選未寫入 Host；副 Pane GUI 三列拖選高亮 PASS | **T06 正向完成**：Rust Library **524 PASS／0 FAIL／1 Git History 長測 FILTERED**、Native **32/32 PASS**，Windows EXE、fmt／diff PASS。[T06 完整實測](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md)。**下一項 NOW T07**；`supported=false`，未合 main／Push／部署 |
| **T07** | Agent Profile、Agent 自訂執行路徑、狀態錯誤提示 | **部分完成（Windows 六個正常 GUI 子項 PASS）**：① Host Profile 冷啟動／無 Hook 附加不假報登入。[啟動](gpui-t07-agent-profile-acceptance-20261113.md) ② CLI Override 路徑與 Save/Clear。[Override](gpui-t07-agent-executable-override-acceptance-20261113.md) ③ Host 標準 Command Profile 建立／Rename CAS／↑↓ 排序。[基本管理](gpui-t07-host-profile-management-acceptance-20261113.md) ④ Impact 預覽→明確刪除確認，Host revision／引用阻擋與刪後讀回。[刪除](gpui-t07-host-profile-removal-acceptance-20261113.md) ⑤ **Clone selected**：Host CLI 複製完整 Command／Prompt／Description／Quota 及安全 Managed 欄位，新 ID／rev0；GPUI 重複 Copy→Copy 2、Host 4→5→6 並獨立回讀；Managed 降低防護需要另行明確授權，Native 不自動同意。[Clone](gpui-t07-host-profile-clone-acceptance-20261113.md)。⑥ **Routing Description／Quota Group**：Windows GPUI 選取後自動載入、編輯與明確清空，Command rev0→1→2、Managed rev0→1；Host 獨立讀回 Command／Prompt／Managed config 保留，未啟動 Provider。[Routing](gpui-t07-host-profile-routing-editor-acceptance-20261113.md)。Library **538 PASS／0 FAIL／1 FILTERED**、Native **33/33 PASS**、Windows EXE PASS，CLI conformance **3/3 PASS** | **NOW T07**：Classic Command／Prompt 與 Managed 進階 Edit／模型 Persona、真實未登入／過期 Provider UX；Clone 目前來源版本檢查不是 Host 單一交易的原子 Clone，真被引用刪除阻擋 GUI 尚未驗收；`supported=false` |

**F1 結案條件：** 不依賴特定測試工具才能新增 Terminal／Agent；使用者從主、副窗格都能完成日常工作，且保留 Host-owned PTY。

### F2｜Editor／檔案總管／搜尋

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **E01** | Explorer 單擊預覽、雙擊／Quick Open 正式開檔 | **部分完成**：先前有重複 Tab 問題，歷史分支曾修正單 Pane；本整合分支需核對 | `README.md` 預覽→雙擊→`Ctrl+P` 重開同檔，不產生意外兩個可編輯 Tab；Dirty 不遺失 |
| **E02** | Editor 文字輸入、尋找／取代、Undo／Redo、`Ctrl+S`、Save As | **部分完成**：GUI 編輯存檔和磁碟讀回有實測；Find/Replace 有程式與測試 | 一個繁中 UTF-8 檔依序編輯、尋找、取代、Undo、儲存、重新開啟，檔案結果一致 |
| **E03** | Explorer 開檔、建立／重新命名／複製／剪下／貼上／拖放 | **已有實作／待操作驗收**：有 Explorer 安全操作與 OS 拖放底層 | 用隔離目錄完成上述常用動作，畫面與實際檔案一致；不需做罕見惡意路徑壓測 |
| **E04** | `Ctrl+P`、Explorer 篩選與全文搜尋；快速定位檔案 | **部分完成**：Quick Open、模糊搜尋／內容搜尋有實作，長檔名 GUI 部分驗收 | 在含巢狀／同名檔的隔離專案搜尋、開啟、顯示相對路徑並定位 |
| **E05** | 跨 Worktree 內容搜尋與結果開啟 | **待核對**：現有內容搜尋的跨 Worktree 範圍需與 Classic 比對 | 確認搜尋範圍選項，結果可開到正確 Worktree，不誤開當前工作樹同名檔 |
| **E06** | 多 Editor Pane、Dirty／草稿保存、關閉／重開復原 | **部分完成**：Rust 身分／Undo／草稿回歸已有；GUI 部分驗證 | 左右各開一份檔案、新增草稿、儲存／重開；未儲存內容與檔案關聯正確 |

**F2 結案條件：** 使用者可以長時間進行正常文字編輯、搜尋與檔案管理，而不需要回到 Classic 處理同類工作。

### F3｜Git（日常版控優先，再做進階）

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **G01** | Source Control：Changes／Staged／Untracked、搜尋與多選 Stage／Unstage | **已有實作／待操作驗收**：Git area、Ctrl/Shift 批次模型已在 Native | 隔離 Git repo 修改兩檔→多選 Stage／Unstage→看到正確 Index/Working Tree |
| **G02** | **中央 Git Diff**、Hunk 導航、原檔／Diff 切換 | **部分完成（已有實際 GUI 證據）**：中央 Diff 已整合，部分導航正常 | 正常點擊不同變更檔、原檔、下一個 Hunk；檢查顯示一致與側欄不擁擠 |
| **G03** | Commit、Amend、歷史瀏覽與 Commit 預覽 | **已有實作／待操作驗收**：後端安全 Preview／Commit／Amend 已存在 | 只在隔離 repo 正常 Stage→輸入訊息→Commit→History 讀回 |
| **G04** | Git Graph 的 Ref／Commit 列表、分支選單、差異比較 | **已有實作／待操作驗收**：Ref 搜尋、分頁、比較、拓撲、Commit 選單程式已存在 | 開 Graph，選分支比較並開原檔；按 Classic 分支/Commit 選單逐項補接 |
| **G05** | 指定分支 Pull／Push、Merge 進目前分支但不切換 | **部分完成**：安全 Pull／Push／Merge Preview 與部分右鍵入口已存在；操作入口整合未完成 | 完成分支下拉與 Git Graph 的一致入口，隔離 repo 讀回合併結果，不偷偷 Checkout |
| **G06** | Worktree 管理、建立／切換／清理、合併衝突處理 | **部分完成**：Worktree 及衝突選邊模型存在；完整 Classic 操作仍待核對 | 一般新建／開啟／切換及簡單衝突解決 UI 閉環；LLM 協助合併另列後續功能，不阻塞日常 Git |

**F3 結案條件：** 使用者可在 Native 完成一般版本控制操作，中央 Diff 與 Graph 不只是唯讀展示。

### F4｜Settings／Classic 共用 Rust Core

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **S01** | Classic 設定分類、每一列是真功能或清楚顯示未提供 | **部分完成**：設定分類與語言選項已在 GPUI，但仍有 preview／Unavailable 項 | 盤點各分類與每項開關：`可讀寫／唯讀／尚未接線`，先接常用項，不保留假按鈕 |
| **S02** | Classic／GPUI 共用設定只存 **Rust Core／Host** | **部分完成**：共用契約／CAS 與 Native 私有差異已建立，尚有待遷移項 | 將兩版都使用的設定接同一份 Rust 真相，Native/Flutter 不各自重複維護 |
| **S03** | 兩版同時開啟、修改與重啟後設定互通 | **已有實作／待正常操作驗收**：Host 設定 CAS 有回歸，但雙 GUI 日常互通仍不完整 | 使用隔離相容 Host：Classic 修改→GPUI 讀回；GPUI 修改→Classic 讀回→重啟 |
| **S04** | i18n 設定實際影響所有設定頁文字及選單 | **部分完成**：語系選擇器、部分 Classic 對應語彙已存在 | 切換繁中／英文，檢查主要分類、說明、操作按鈕與儲存後重開；不只改下拉值 |

**F4 結案條件：** 常用設定可真正生效且跨 Classic/GPUI 一致，唯 UI 特有偏好允許單獨保留。

### F5｜Agent Hook 狀態與通知

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **H01** | `working／waiting／blocked／done` 在正確 Agent／Tab 顯示 | **部分完成**：Host Presence 模型、快照／變更投影與 Tab Status 已有 | 用正常的 Codex／Claude 操作檢查狀態變化與 Tooltip，不需要強制崩潰 |
| **H02** | Hook 開關與通知偏好能儲存、讀回 | **部分完成**：Codex／Claude Hook CAS 及設定 UI 有接線；T03 真實 Devin TUI 出現原有 JBContext 使用者 Hook failed（不阻塞 CLI 回覆），原因未判定、未改正式 Hook | 在隔離 Host 開關一次→讀回→重開設定頁一致；其他 Provider 逐項擴充，並釐清 Devin 既有 Hook 失敗是否需要相容修正 |
| **H03** | Windows 原生通知與點擊後定位到正確 Pane／Session | **部分完成**：Toast adapter 與通知去重規則已有實作；真實 GUI 通知尚未驗收 | 正常等待／完成情境看到通知，點擊定位；Classic/Native 不同時重複提醒 |

### F6｜UI／UX：最後做一致性收斂

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **U01** | Classic Mixed Tab／左右 Pane／`+`、右鍵與釘選 | **部分完成（左右 Tab 列對齊已實測）**：GPUI 原先主 Pane Header 在分割容器外、副 Pane 在內而有高低差；現在左右 Tab 同列、共享 pinned row，高度對齊且副 Pane 不再重複顯示第二組 Tab。Windows 3440×1440 實機 PASS；[本輪驗收](gpui-terminal-fullpane-split-tab-alignment-acceptance-20261112.md) | 其餘單／雙 Pane Classic 操作、分割方向、密集 Tab 的日常一致性仍在 U01 驗收，不因這次局部修正就整項結案 |
| **U02** | Quick Open／檔案樹／Tab 的長檔名與資訊密度 | **部分完成**：長路徑搜尋可顯示；長 Tab 可能擁擠 | 完整檔名與路徑可辨識、列表間距合理、常用按鈕不被擠掉 |
| **U03** | 鍵盤焦點、一般 Windows 注音、快捷鍵 | **部分完成**：T06 Terminal 雙 Pane 真實注音候選／選字已通過，Editor／搜尋 IME 焦點與鍵盤 UX 尚須 U03 驗收 | 編輯器、搜尋及 Terminal 正常輸入繁中，快捷鍵不搶組字、不吃大寫 |
| **U04** | 正常重開後 Workbench／Tab／草稿／釘選／設定維持 | **部分完成**：Host Session 及部分佈局重啟已有實測 | 在隔離 Workspace 關閉重開，日常佈局及資料仍正確，不需要測極端斷電 |
| **U05** | **Agent 額度狀態列與 Classic 介面密度、圖示、顏色、刷新一致** | **部分完成（Windows 合成 UI＋真正隔離 Host CLI 釘選 CAS PASS）**：維持 5 分鐘額度刷新、Classic 三色門檻、Provider 圖示與多帳號精簡標籤、可點擊明細／Refresh／AGY 五項與固定面板工具列。[原 UI 驗收](gpui-u05-agent-quota-status-classic-parity-20261113.md)。**修正單一帳號刷新失敗原本誤標所有 Provider stale**；額度視窗開啟時共享釘選輪詢 3 秒、閒置 15 秒；發現 CLI `settings set` 原本禁止 JSON 陣列，已改成僅對 `agents.quotaUnpinnedKeys` 接受經 Rust Core 規範驗證的陣列。新版 CLI 隔離 Host 實測釘選 CAS 寫入／第二條連線讀回／取消 AGY／舊值拒絕／非法值拒絕／重啟後持久化均 PASS，正式 Host 未碰。[U05 實測](gpui-u05-quota-popup-refresh-and-pin-sync-20261113.md)。GPUI Library **540 PASS／0 FAIL／1 FILTERED**、Native **33/33 PASS**、Rust Core **136/136 PASS**、Windows CLI／GPUI EXE build PASS | **仍待** 正式 Classic＋GPUI 雙 GUI 配新版 Host 實際釘選同步、逐個真 Provider 額度回應、Codex Reset 等進階操作；Rust CLI 雙 Client 被忽略測試的 Binary 編譯逾時不算 PASS。`supported=false` |

## 4. 確定的執行隊列（每次只讓一項成為 NOW）

| 順序 | 工作單 | 本批要交付的結果 | 狀態 |
| --- | --- | --- | --- |
| DONE | **T01** | GPUI 主／副 Pane 由 `+` 建立真正可互動 Terminal；完成一般 GUI 正向流程 | **Windows GUI + Host 讀回 PASS**；[驗收紀錄](gpui-t01-terminal-two-pane-acceptance-20261112.md) |
| DONE | **T02** | Agent 自動偵測、刷新、未安裝提示及 Classic 下拉選單操作一致 | **Windows 主／副 Pane GUI PASS + 513/513 Rust Library PASS**；[驗收紀錄](gpui-t02-agent-cli-menu-acceptance-20261112.md) |
| DONE | **T03** | 六種已安裝／已登入 Provider 的 New Agent 真實啟動與互動 | **Windows GUI + Host read 6/6 PASS**；[驗收紀錄](gpui-t03-live-six-provider-acceptance-20261112.md)。未登入錯誤提示移 T07 |
| DONE | **T04** | Attach 既有 Host Session、Tab 切換／關閉／主副 Pane 重新 Attach、GUI 重開保留 Session | **Windows GUI + Host 2/2 PASS**；[驗收紀錄](gpui-t04-attach-session-two-pane-acceptance-20261112.md)，原 Pane 位置持久化移 T05 |
| DONE | **T05** | Host Pane 持久化、真實 Claude Agent 跨 Pane 拖曳焦點、乾淨 Editor／Terminal 批次關閉 | **Windows GUI + 隔離 Host PASS**；Library **516/516**、Native **31/31**；[T05A](gpui-t05-host-pane-persistence-tdd-20261112.md)／[T05B](gpui-t05b-agent-tab-and-bulk-close-acceptance-20261112.md) |
| DONE | **T06** | Windows 真正注音候選／選字及 Host 提交，雙 Pane 中文輸入、普通鍵盤快捷鍵、副 Pane 多列選取 | **2026-11-13 Windows GUI／隔離 Host PASS**：主 Pane 候選「你」，副 Pane 方向鍵選「擬」，Host 獨立讀回均正確、無 `su3`，未提交候選跨 Pane 失焦取消沒有 PTY 輸出；副 Pane 拖選 3 列 GUI 高亮 PASS。Library **524 PASS／1 FILTERED**、Native **32/32 PASS**、EXE PASS。詳見 [T06 實測](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md) |
| INTEGRATED／待 GUI 驗收 | **T07** | Host Profile、Command／Custom Prompt、Managed Model／Persona、Provider 安全提示已整合 GPUI 分支；維持正常功能優先 | `7c300ec04`：整合測試 Managed 2/2、選項探索 4/4、Provider 提示 1/1、Native 34/34、Windows cargo check PASS。未登入／Token 過期的 Provider typed status、實際 GUI 編輯待後續單獨驗收；**不再阻塞 E01/E02**，`supported=false` |
| INTEGRATED／待 GUI 驗收 | **E01** | Explorer 預覽、正式開檔、Quick Open、同檔 Dirty Buffer/Undo，修正注音 Enter 和 Editor 開檔確認 | `15c15b496`：E01 Rust 2/2、Native 36/36、Windows cargo check PASS。真 GUI 點擊/注音候選保留驗收，但不因其未測而停住後續日常功能 |
| **NOW** | **E02** | Editor 文字編輯／尋找取代／Undo Redo／存檔／Save As 的正常閉環 | 用隔離 UTF-8 檔執行編輯→查找→取代→復原→儲存→重開讀回；先修實際缺口，再進入 E03 |
| NEXT | E03 → E04 → E05 → E06 | Explorer 檔案管理、快速搜尋、跨 Worktree、草稿多 Pane | 排隊 |
| NEXT | G01 → G02 → G03 → G04 → G05 → G06 | Source Control／中央 Diff／Graph／分支／Worktree | 排隊 |
| NEXT | S01 → S02 → S03 → S04 | Settings 接線、Rust 共用設定與語系 | 排隊 |
| NEXT | H01 → H02 → H03 | Hook 四態、設定、Windows 通知 | 排隊 |
| NEXT | U01 → U02 → U03 → U04 | UI／UX 功能可用性與 Classic 操作一致 | 排隊 |

**排程規則：** 該工作包完成基本正向 UI 驗收就移到下一項，不因極端案例未驗證而反覆停留；發現正常情境確實壞掉才回頭修。某項已有正常操作 PASS 可縮短，立即移到下一項。保留安全回歸，不以省時間為由刪除原有測試。

**2026-11-13 NOW 調整（避免 T07 驗收無限阻塞）**：使用者明確要求繼續功能實作，不再把所有真實 Provider 未登入／逾期與 GUI 進階驗收列為開發排隊的前置條件。已在 GPUI **整合分支**提交 T07 功能 `7c300ec04`（Managed 2/2、Model/Persona 4/4、Provider 1/1、Native 34/34、cargo check PASS），以及 E01 `15c15b496`（預覽／正式開檔／同檔 Dirty Buffer/Undo 2/2、Native 36/36、cargo check PASS）。**這不是聲稱 T07／E01 GUI 實機 DONE**：Windows 實際操作與真 Provider typed auth 另列後續驗收，原矩陣的歷史證據不覆蓋。**NOW 改 E02**，保留 `supported=false`，`main`／Push／部署／Release 均不動。

## 5. 每次「繼續／進度」的更新規約

1. 開始前記錄 **本輪 HEAD、工作樹狀態、NOW ID**；僅修改所需檔案。保留 protected smoke Git fixture、使用者 WIP，不自行清理。
2. 先檢查程式與已整合 Commit；**有功能就驗收／補 UI 接線**，不要把舊文件中的「未完成」照搬成現在的缺失。
3. 有變更才做精準 TDD／專項回歸；正常用例在隔離 GUI／Host／Git repo 操作，避免大規模故障注入與不必要的完整重編。
4. 每個工作包完成時更新表格：`狀態`、`通過的正常流程`、`剩餘缺口`、`Commit`、`下一個 NOW`。未取得 GUI 操作證據標示 **待操作驗收**，不能因單測綠燈改成完成。
5. 優先順序若改動，只修改本檔第 4 節並在進度紀錄寫原因。持續以這份檔案當下一輪的工作入口，不重複規劃、也不每次都重問需求。
6. 每一個功能工作包有自己的提交；本次建檔只提交規劃／追蹤，不混入無關業務程式碼。

### 進度紀錄（新增一列，不覆蓋歷史）

| 日期 | HEAD／Commit | 工作單 | 本輪成果及驗證 | 現況／接續 |
| --- | --- | --- | --- | --- |
| 2026-11-12 | `789f2fa00`（建檔前） | PLAN | 重新聚焦六大功能線；依已整合程式、既有文件建立首次盤點，正常 GUI 流程與極端測試分開管理 | **NOW T01**；本輪是計畫落地，未宣稱新功能完成 |
| 2026-11-12 | `165c79824`（本輪基線；T01 驗收 Commit 為本列所在提交） | T01 | 隔離 Windows GPUI 主、副 Pane `+` 建立兩個 `cmd.exe`；GUI `echo gpuit01primaryok`／`echo gpuit01secondaryok` 均 PASS；Host `terminal list/read` 確認不同 Session／Tab ID、互不串流。GUI 正常關閉、僅測試 Host 停止、fixture Git 狀態未變；既有完整 Cargo 回歸本輪未重跑。[詳見 T01 紀錄](gpui-t01-terminal-two-pane-acceptance-20261112.md) | **T01 完成（正常流程）→ NOW T02**；`supported=false`、不合 main／Push／部署 |
| 2026-11-12 | `6ce141b79`（T02 基線；T02 修改的 Commit 為本列所在提交） | T02 | Classic 與 Native 共用 Agent descriptor 的名稱與順序；Native 去除多餘 `New Agent ·` 前綴。TDD RED→GREEN，新測試 **1/1 PASS**；GPUI Library **513/513 PASS**；Windows binary build／fmt PASS；實際 Windows 主、副 Pane 六種 Agent、刷新和獨立 PATH 空清單 PASS，沒有啟動 Host。見 [T02 驗收](gpui-t02-agent-cli-menu-acceptance-20261112.md) | **T02 完成（Windows 正常流程）→ NOW T03**；未合 main／Push／部署，`supported=false` |
| 2026-11-12 | `edaa07305`（T03 基線；T03 驗收 Commit 為本列所在提交） | T03 | Windows GPUI 實際由 `+` 啟動六種已登入 Provider，均從真實 TUI 收到簡短無工具回覆；隔離 Host `terminal list/read` 交叉確認六個獨立 running Session，輸出無串流；正常關閉 GUI，僅停隔離 Host，fixture 狀態前後相同。詳細見 [T03 驗收](gpui-t03-live-six-provider-acceptance-20261112.md)。未登入情境與 Devin Hook/conhost 警告另行追蹤；本輪無程式改動、未重跑 Cargo | **T03 六種正向完成 → NOW T04**；`supported=false`，未合 main／Push／部署 |
| 2026-11-12 | `30d5db67a`（T04 基線；T04 驗收 Commit 為本列所在提交） | T04 | Windows GPUI 主／副 Pane 以 `+` Attach 原先關閉預覽的兩個 Host Session，輸入標記與完整輸出保留；GUI 關閉重開後兩個 Session／Tab ID 不變且仍能輸入，Host 讀回無串流或重複 PTY；獨立 Host 已停止、fixture 原狀。**額外實機發現重啟丟失 Host 雙 Pane 分配**，移 T05；見 [T04 驗收](gpui-t04-attach-session-two-pane-acceptance-20261112.md)，本輪無產品程式改動、未重跑 Cargo | **T04 一般 Attach／Session 管理完成 → NOW T05 修復 Pane 持久化及完成 Tab 操作**；`supported=false`，未合 main／Push／部署 |
| 2026-11-12 | `38c93f54b`（T05A 基線；本列隨 T05A 實作提交） | T05A | Rust V1 版面持久化新增 Host 三重身分與原 Pane/焦點；首次繪製等待 Host 認證清單。TDD RED→GREEN、Rust Library **515/515 PASS**、Native binary **31/31 PASS**、Windows 建置 PASS。隔離 GUI 兩 Terminal 主／副 Pane 重啟後位置與原 Session 不變、兩邊 echo 無串流；右鍵／跨窗格拖曳、Pin、Close to right PASS，未儲存 Editor Close Others 遭安全保護。Codex 更新提示誤選已 Ctrl+C，版本查仍 `0.167.0`；[T05 詳錄](gpui-t05-host-pane-persistence-tdd-20261112.md) | **T05A 完成；NOW T05B** 真 Agent 與乾淨預覽批次操作；`supported=false`，未合 main／Push／部署 |
| 2026-11-12 | `cf9d8d928`（T05B 基線；T05B 修正 Commit 為本列所在提交） | T05B | Windows GPUI 由 `+` 啟動已登入 Claude Code v2.1.322 與普通 cmd；真實跨 Pane 拖曳發現選取／TUI 焦點未同步，補 `post_cross_pane_drop_focus` 與 `focus_mixed_tab` 接線，TDD RED→GREEN。新建置 GUI 重新 Attach 相同 Host，拖曳即顯示正確 Claude TUI、Host ID／數量不變。實測乾淨 Native Editor 及兩個 cmd 的 Close Others／Right、右鍵移動；Host 全程 3 個 running PTY，fixture Git 狀態不變；[T05B 驗收](gpui-t05b-agent-tab-and-bulk-close-acceptance-20261112.md)。Rust Library **516/516 PASS**、Native Binary **31/31 PASS**、fmt 與 build PASS，最後只停隔離 Runtime | **T05 全部正常流程完成 → NOW T06**；`supported=false`，未合 main／Push／部署 |
| 2026-11-12 | `37b6010ef`（本輪起始 HEAD；滿版修正 Commit 為本列所在提交） | T06／U01 子項 | 修正 Host/VT 終端尺寸上限 512×200、Host 成功 Resize 本地格線同步、400ms 獨立檢查；左右 Tab 同一 header row 並共享 pinned 高度，移除副 Pane 重複 Header；Windows 3440×1440 實機兩 Pane Tab 對齊、Terminal 全高度／背景填滿 PASS，隔離 Host 兩個 running Session 左右標記互不串流。[本輪詳細紀錄](gpui-terminal-fullpane-split-tab-alignment-acceptance-20261112.md)。Rust Library **519/519 PASS**、**1 項無關 Git History 長測 filtered**，Native **31/31 PASS**，Windows EXE 編譯、fmt／diff check PASS。GUI 鍵盤焦點本輪未完成，不當作 T06 整項 PASS | **T06 Resize 與 U01 Tab 對齊子項通過，NOW 仍 T06**；隔離 GUI 關閉、Host 已停，fixture 原狀；`supported=false`，未合 main／Push／部署 |
| 2026-11-12 | `2c93569c5`（本輪起始 HEAD；遷移 Commit 為本列所在提交） | T06 Terminal Core 子項 | GPUI `VtScreen` 保留上層介面，內部 `vt100 0.18.1` → `alacritty_terminal 0.26.0`、停用 OSC52 clipboard 事件、保留 ANSI 16／256／truecolor、游標、繁中寬字元、ConPTY Resize。13/13 專項 PASS、Library **520 PASS／0 FAIL／1 Git History 長測 FILTERED**、Native **31/31 PASS**、Windows build／fmt PASS。隔離 GUI 真實 cmd `echo alacritty_shell_ok` 與 Claude Code v2.1.322 TUI 雙 Pane，Host 兩 Session 各自輸出，正常關閉 GUI 並僅停止隔離 Host，fixture 原樣；[完整紀錄](gpui-alacritty-terminal-core-migration-20261112.md) | **GPUI 引擎替換 Windows 正向完成，NOW 仍 T06** IME／Clipboard／快捷鍵；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `086cc62d5`（本輪起始 HEAD；Clipboard 修正 Commit 為本列所在提交） | T06 鍵盤／剪貼簿子項 | GPUI 統一 Clipboard shortcut router：主／副 Pane Ctrl+Shift+C 正確複製，Ctrl+V 與 Ctrl+Shift+V 導向 Host，Alacritty BRACKETED_PASTE 模式包裝且不超過 4 KiB，Ctrl+C 保留 PTY 中斷；Secondary 文字列可選取與複製，選取上限 200。TDD RED→GREEN，新案例 **2 PASS**；Library **522 PASS／0 FAIL／1 FILTERED**、Native **31/31 PASS**、Windows EXE、fmt／diff check PASS。隔離 Windows GUI 兩個 cmd PTY 的 Echo、兩種貼上、雙向 Ctrl+Shift+C 剪貼簿讀回與繁體 Unicode 正確，普通 Ctrl+C 後 Session 仍能輸入；剪貼簿原文字以 DPAPI 暫存並還原，測試 GUI／Host 已停止、fixture 原樣。[T06 實測](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md) | **T06 鍵盤與 Clipboard 正向完成；NOW 仍 T06** 等待真正注音候選字驗收（模擬注音鍵僅產生 ASCII，不當作已證實 IME 故障或成功）；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `725580ac3`（本輪起始 HEAD；IME 分流修正 Commit 為本列所在提交） | T06 注音 marked preedit 保護 | 主、副 Terminal 鍵盤共用 `TerminalKeyDispatch`：組字標記尚在時注音 ASCII、候選空白／Enter／Backspace、Ctrl+Shift+C／V 不送 Host，已提交 Unicode 仍走原生 TextInput；無組字時保留 Ctrl+C／Clipboard 行為。TDD RED（E0432／E0425）→ GREEN；Library **523 PASS／0 FAIL／1 Git History 長測 FILTERED**、Native **31/31 PASS**、Windows EXE、fmt／diff check PASS。[T06 本輪增補](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md)。Windows 桌面有其他前景程式使用，未切換輸入法／搶焦點，**真注音候選窗及選字未實機驗收** | **NOW 仍 T06** 中文模式候選／選字、首鍵、失焦與雙 Pane GUI；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `736f7dbd1`（本輪起始 HEAD，注音 first-key 防護與 GUI 驗收提交隨本列） | **T06 正常流程結案** | Windows Query-only IMM 的 open+native conversion 模式保護尚未收到 marked preedit 的第一個 ASCII 注音鍵，英數模式與 Ctrl+C 保持原邏輯；TDD RED → GREEN。Library **524 PASS／0 FAIL／1 FILTERED**、Native **32/32 PASS**、Windows build／fmt／diff PASS。另建新隔離 Runtime／State，Windows Microsoft Bopomofo `S,U,3` 主 Pane 候選「你」及副 Pane `↓` 選「擬」後，兩個 Host 分別收到對應 Unicode、沒有 raw `su3`；副 Pane 滑鼠拖選 `T06_DRAG_A/B/C` 三列 GUI 高亮；在副 Pane 再開候選切回主 Pane，TSF cancel 且 Host cursor=1501 未增（0 bytes）。GUI PID 23632 關閉、隔離 Host 已 stop／running=false；fixture 原有 `M` 與 `??` 原狀。[完整 T06 實測](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md) | **T06 DONE → NOW T07**；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `34df0459b`（本輪起始 HEAD；T07 變更 Commit 為本列所在提交） | **T07 Profile 冷啟動／Hook 無回報子項** | Host Profile 原本須先有 Workspace，否則 GUI 顯示未註冊專案，且缺 Hook 的 Host PTY 無法附加。新增明確 Profile Launch 的安全 Workspace 註冊（不產生多餘 PTY）、精確 Host 工作樹／Tab／Agent 類型核對，以及「Provider sign-in not verified」提示。TDD RED（E0061、E0599）→ GREEN；Library **526 PASS／0 FAIL／1 長測 FILTERED**、Native **32/32**、EXE／fmt／diff PASS。隔離 GUI 從 0 Session 直接 Launch 一份 `cmd.exe` Profile，Host 僅 1 running Session；GUI 輸入 `echo T07_PROFILE_INTERACTIVE_OK` 並由 Host 讀回。隔離 GUI 關閉、Host 停止後確認 `running=false`，fixture WIP 原狀。[T07 詳錄](gpui-t07-agent-profile-acceptance-20261113.md) | **NOW 仍 T07**：真實未登入／過期憑證、Override 錯誤與 CRUD；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `606049563`（本輪起始 HEAD；本地執行檔修正 Commit 為本列所在提交） | **T07 Native Agent 執行檔覆寫子項** | 修正 Windows 真實存在 `.txt` 被存成 Agent CLI、舊無效覆寫出現在 New Agent 選單、空白 Save 意外清除路徑；只接受 `.exe/.com/.cmd/.bat`，與 Rust Native CAS/掃描器共用檢核，UI 顯示副檔名錯誤、CAS 衝突與明確 Clear。TDD RED（`.txt` 被保存、E0425）→ GREEN；Library **528 PASS／0 FAIL／1 FILTERED**、Native **32/32**、Windows EXE、fmt／diff PASS。隔離 Windows GUI 實測不存在 `.exe`／存在 `.txt` 拒絕、合法 `.cmd` Save＋磁碟讀回、錯誤重存保留舊值、空白 Save 不清除、Clear 恢復 Auto PATH；三次測試 GUI 關閉，fixture WIP 保留。[完整實測](gpui-t07-agent-executable-override-acceptance-20261113.md) | **NOW 仍 T07**：真實 Provider 未登入／過期驗收與 Host Profile 建立／編輯／排序／移除；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `feaf52253`（T07 本輪起始 HEAD，管理子項 Commit 隨本列提交） | **T07 Host Profile 建立／Rename／↑↓ 排序** | GPUI 新增標準 Command Profile 建立、選擇共用 Descriptor Adapter、Host revision CAS 重新命名及全清單 revision CAS 排序；UI 只保留 id/name/type/mode/revision，不保留 Host 命令或機密。TDD RED（缺 revision、CreateCommand/Rename/Reorder）→ GREEN。隔離 Windows GUI Host 從 0 筆建立 Claude（初始 rev=0；首次遇 GPUI 誤拒後修正）及 Codex；Rename Claude rev0→1；Host stale rev0 更新被拒；↑↓ 雙向排序並獨立讀回、2 筆 ID/預設命令保留。Library **531 PASS／0 FAIL／1 FILTERED**、Native **32/32**、Windows EXE、fmt／diff PASS；GUI 全關閉、僅隔離 Host 停止且 `running=false`，fixture WIP 原樣。[完整實測](gpui-t07-host-profile-management-acceptance-20261113.md) | **NOW 仍 T07**：Managed/進階完整編輯、Clone、刪除含 Impact、真 Provider 未登入／過期憑證；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `011ffd8f2`（本輪起始 HEAD；刪除子項 Commit 為本列所在提交） | **T07 Host Agent Profile 刪除與 Impact 預覽** | 新增 Host `removal-impact` 的精確 ID／Revision 安全解析，僅保留引用數量；GPUI 兩階段 Review impact → Confirm delete selected，Automation／Tab／ExecutionPolicy 引用阻擋確認。刪除前 Host 再讀最新 Impact 比對，送 `--expected-revision` 與 `--confirm`，刪除後 Host 清單回讀必須不再含原 ID。TDD RED（E0425）→ GREEN；Library **533 PASS／0 FAIL／1 FILTERED**、Native **32/32 PASS**、Windows EXE、fmt/diff PASS。隔離 Windows GUI 一筆 `cmd.exe` Profile 由 1→0，CLI 獨立驗證已刪；GUI 關閉、隔離 Host `running=false`、fixture WIP 未動。[T07 刪除完整驗收](gpui-t07-host-profile-removal-acceptance-20261113.md) | **NOW 仍 T07**：Classic 進階 Edit／Managed／Clone、真正未登入／過期 Provider UI；阻擋引用僅有 Rust 測試、未做真 GUI 引用實測；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `aba5ffa0d`（本輪起始 HEAD；Clone 子項 Commit 為本列所在提交） | **T07 Host Agent Profile Clone** | CLI 新增 revision-checked `agent-profile clone`，CLI 讀來源完整 Host Profile（command、Managed config、custom prompt、description、quota group），GPUI 只傳 ID／revision／name，不帶機密欄位，安全投影驗證新 ID 與 rev0；Managed 降低防護 Clone 需 `--confirm-reduced-protections`，Native 不隱式確認。TDD RED（舊 CLI 無 clone／E0425 缺命名 helper）→ GREEN；CLI conformance **2/2 PASS**（首次測試 Host 自動啟動逾時，修測試 harness 後通過）。GPUI Library **535 PASS／0 FAIL／1 FILTERED**、Native **32/32 PASS**、EXE／fmt／diff PASS。隔離 Windows GUI 4→5→6 筆，Copy／Copy 2、各欄位獨立 Host 回讀相同；Managed 風險被 UI 拒、Host 筆數未變；GUI 關閉、隔離 Host running=false、受保護 fixture WIP 原樣。[Clone 紀錄](gpui-t07-host-profile-clone-acceptance-20261113.md) | **NOW 仍 T07** Advanced Edit／Managed／Provider Auth；來源 revision 檢查非原子 Host Clone，`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `5698c981a`（本輪起始 HEAD；U05 額度狀態列改進隨本列提交） | **U05 Status Bar Quotas Classic parity** | 與 Classic `agent_quota_status_bar_content.dart` 比對：恢復每 5 分鐘更新、正確紅／橘／綠門檻、過期資料降權、Provider gauge＋帳號別名、精簡狀態列與點擊詳情、新增底部 Refresh 圖示；Overview 380×480、固定刷新／關閉、AGY 5 桶自動換行。TDD RED 缺少帳號 label 函式（E0425）→ GREEN，Library **536 PASS／0 FAIL／1 FILTERED**、Native **33/33 PASS**、Windows EXE／fmt/diff PASS。僅 Debug+明確 Fixture 的隔離 Windows GUI 驗收 10 Provider/13 Accounts、AGY 五項與低額度 6% 警示色、窄視窗總覽固定 header、刷新圖示觸發 DEMO AGY 明細；兩個 GUI 關閉、受保護 fixture WIP 未改。[U05 紀錄](gpui-u05-agent-quota-status-classic-parity-20261113.md) | **T07 仍 NOW**：真正 Host 每 Provider 刷新與跨 Classic 釘選同步另驗，`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `c9eded39b`（T07 Routing 本輪起始 HEAD；Routing 子項 Commit 隨本列提交） | **T07 Host Agent Profile Routing 編輯／Classic 部分對齊** | 新增 Host `show` 的 Secret-free Routing 投影與 revision-checked `update`，GPUI Settings 兩個輸入欄位 Load/Save、選取及修訂版本變更時失效，不寫入 Native Shared Settings，保留 Command/Prompt/Managed Config。TDD RED（缺 `parse_profile_routing_fields` E0425、缺 Show/UpdateRouting E0599）→ GREEN；隔離 Host CLI conformance **3/3 PASS**（首次新測試將 Description clear 誤當 null，修正為原生空字串，Host 無需更動）；GPUI Library **538 PASS／0 FAIL／1 FILTERED**、Native **33/33 PASS**、Windows EXE／fmt PASS。實機 Command rev0→1→2（修改與清空）、Managed rev0→1，獨立 CLI Show 均確認其他 Host 欄位不變，GUI 關閉、隔離 Host 最終 running=false、受保護 WIP 原樣。[Routing 紀錄](gpui-t07-host-profile-routing-editor-acceptance-20261113.md) | **T07 仍 NOW／部分完成**：待 Command／Prompt 與 Managed 模型 Persona、真實 Provider Auth；`supported=false`，未合 main／Push／部署 |
| 2026-11-13 | `168a4d518`（U05 額度彈出 Host Sync 本輪起始 HEAD；修正隨本列所在提交） | **U05 Quota Popover Single Account Refresh＋Classic Pin CAS** | TDD RED E0425 缺 `mark_failed_quota_refresh`／`shared_quota_pin_poll_seconds` → GREEN：單一帳號刷新錯誤只標該帳號 stale；Quota Popover 開啟時 Host 共享 Pin 每 3 秒讀取、閒置 15 秒。隔離 Host 實測揭露 CLI `settings set` 本身拒絕任意陣列，導致 GPUI Pin／Unpin 不能儲存；新增 Rust Core `cli_set_value_allowed` 僅允許合法 `agents.quotaUnpinnedKeys` 陣列，其他路徑仍純量。Core 136/136、GPUI Library **540 PASS／0 FAIL／1 FILTERED**、Native **33/33**、Windows CLI／GPUI Build／fmt PASS。真實隔離 Host：第一次 CLI 寫三項 Pin、第二條連線讀相同值、解除 AGY、舊 CAS 被拒、非法 Pin 被拒、Host 重啟持久化兩項 Pin；全部 PASS，測試 Host 已關閉。獨立 Rust Bin 雙連線單元測試編譯逾時，非 PASS。[U05 根因與驗收](gpui-u05-quota-popup-refresh-and-pin-sync-20261113.md) | **U05 部分完成，T07 仍 NOW**：正式雙 GUI Host 同步／全部真 Provider 仍待，`supported=false`，未合 main／Push／部署 |

## 6. 既有實作／Classic 規格來源（保留歷史證據）

- **Classic 互動契約：** [gpui-flutter-classic-ux-parity-20261108.md](gpui-flutter-classic-ux-parity-20261108.md)、[gpui-flutter-interaction-parity-20261108.md](gpui-flutter-interaction-parity-20261108.md)。
- **最新常用 UI 缺口初盤：** [gpui-native-ui-ux-improvement-plan-20261110.md](gpui-native-ui-ux-improvement-plan-20261110.md)；早期的 PLAN／FAIL 是該版歷史，不直接代表今天仍失敗。
- **Terminal／Agent：** [gpui-t01-terminal-two-pane-acceptance-20261112.md](gpui-t01-terminal-two-pane-acceptance-20261112.md)（真實 GUI `New Terminal` 雙 Pane 驗收）；[gpui-t02-agent-cli-menu-acceptance-20261112.md](gpui-t02-agent-cli-menu-acceptance-20261112.md)（Agent 偵測、空清單、刷新與 Classic 名稱對齊）；[gpui-t03-live-six-provider-acceptance-20261112.md](gpui-t03-live-six-provider-acceptance-20261112.md)（六個真實 Provider、主副 Pane／Host Session 核對）；[gpui-t04-attach-session-two-pane-acceptance-20261112.md](gpui-t04-attach-session-two-pane-acceptance-20261112.md)（原 Attach／重啟 Session 身分核對）；[gpui-t05-host-pane-persistence-tdd-20261112.md](gpui-t05-host-pane-persistence-tdd-20261112.md)（T05A 雙 Pane 重啟修復）；[gpui-t05b-agent-tab-and-bulk-close-acceptance-20261112.md](gpui-t05b-agent-tab-and-bulk-close-acceptance-20261112.md)（T05B 真實 Claude 拖曳、焦點與乾淨批次關閉）；[gpui-agent-cli-dropdown-parity-20261112.md](gpui-agent-cli-dropdown-parity-20261112.md)（含 real Codex／Claude、主副 Pane、Host Session）；[gpui-terminal-classic-transport-parity-20261109.md](gpui-terminal-classic-transport-parity-20261109.md)。
- **本輪 Terminal 滿版／Tab 對齊：** [gpui-terminal-fullpane-split-tab-alignment-acceptance-20261112.md](gpui-terminal-fullpane-split-tab-alignment-acceptance-20261112.md)（T06 Resize／U01 雙 Pane Tab 高度修復、Windows GUI 與 Host 雙 Session、回歸檢查）。
- **T06 Windows 鍵盤／剪貼簿：** [gpui-t06-native-clipboard-keyboard-acceptance-20261113.md](gpui-t06-native-clipboard-keyboard-acceptance-20261113.md)（Ctrl+V／Ctrl+Shift+V、雙 Pane Ctrl+Shift+C、Ctrl+C、Unicode 直接提交、IME 候選未核實）。
- **Alacritty Terminal Core 替換：** [gpui-alacritty-terminal-core-migration-20261112.md](gpui-alacritty-terminal-core-migration-20261112.md)（移除 vt100、Alacritty v0.26、Windows cmd/Claude GUI 與 Host 正向驗收；T06 輸入法仍待驗收）。
- **Git 與中央 Diff：** [gpui-central-diff-branch-audit-20261112.md](gpui-central-diff-branch-audit-20261112.md)、[gpui-source-control-git-graph-parity-20261109.md](gpui-source-control-git-graph-parity-20261109.md)。
- **設定／Hook：** [gpui-agent-hook-status-notification-parity-20261111.md](gpui-agent-hook-status-notification-parity-20261111.md)、[gpui-dart-component-reuse-tdd-20261109.md](gpui-dart-component-reuse-tdd-20261109.md)。
- **其他舊里程碑：** [gpui-p0-p1-p2-tdd-closure-20261108.md](gpui-p0-p1-p2-tdd-closure-20261108.md)、[gpui-active-work-ledger-20261107.md](gpui-active-work-ledger-20261107.md)。
- **最新程式回歸（U05 額度彈出真 Host Pin CAS＋錯誤狀態修正）：** GPUI Library **540 PASS／0 FAIL／1 FILTERED**（Git History 既有長測未執行）、Native Binary **33/33 PASS**、Rust Core **136/136 PASS**、Windows GPUI EXE 與新 CLI 編譯 PASS，Rust fmt／diff PASS。TDD 驗證一個帳號刷新失敗僅其讀數 stale；CLI 只容許規範內額度 Pin 陣列。真實隔離 Host 跨不同 CLI 連線的 Pin 寫入、獨立讀回、解除、CAS 衝突拒絕、非法鍵拒絕、重啟持久化 PASS；舊 Rust Bin 雙同時 Client 測試這輪連結 timeout，非 PASS。正式 Classic Host 未升級、兩個桌面 GUI 未做同步實機；T07 仍 NOW，`supported=false`。

**T07 平行支線最新成果（未整合）**：`feat/gpui-sweflash-t07-command-prompt-20261113` 的安全 stdin Host Command／Prompt 通道 `5f0d602e6` 與 Native UI `f6d09cdd8` 已獨立提交。隔離 Host stdin 讀改寫、GUI Module 與 Windows EXE／Rust 回歸已通過；**GUI 實際點擊驗收被工具工作區權限阻擋，仍須驗收，不是 T07 DONE，也不提前合併 E01**。詳見功能分支的 `docs/gpui-sweflash-t07-command-prompt-ui-handoff-20261113.md` 與 [平行計畫](gpui-sweflash-parallel-implementation-plan-20261113.md)。

**T07 Native→隔離 Host 端到端驗證（尚未中央整合）**：`feat/gpui-sweflash-t07-managed-20261113` 新增 `ee60c06e6`，Native Managed Bridge 在 tempfile 一次性 Host 真正驗收 Load／Save／CAS 拒寫／Clear／Show 讀回，Permission Mode 與 Prompt 保留，**1/1 PASS**。另將已驗證的 Provider 安全提示收斂到同一條 T07 feature branch `ee4fa0f82`（只處理匯入區衝突），Provider 1/1、Managed 2/2、探索 3/3、Native Bin 34/34、Windows Debug EXE PASS。這**不是** GPUI 中央整合或 GUI 滑鼠／鍵盤驗收：桌面仍有前景應用，避免搶焦點；真 Provider 認證、實際模型探索與 GUI／IME 待驗。原 NOW=T07、`supported=false`，E01 不提前整合；詳見 [平行計畫](gpui-sweflash-parallel-implementation-plan-20261113.md)。

**T07 Managed／Provider 支線（尚未整合）**：Managed Host-owned stdin 更新 `4d934b687` 通過 CLI 4/4 與隔離 Host 1/1；Native Managed UI `8dfd89405` 通過專項 2/2、Windows EXE/Bin 34/34；唯讀動態 Model／Persona 選項 `5262b589e` 通過解析 3/3、Windows EXE/Bin 34/34。Persona UI 欄位對應 Rust Managed `agent`，不改 Permission／Sandbox；探索限 Classic 支援命令且必須明確點擊，未對真實 Provider 做正向實測。Provider 安全啟動提示 `ba999e6c9` 通過 Rust 1/1 與 GPUI 編譯檢查，不將未知登入狀態誤判成 Token 過期。**可信 Host 認證狀態、Windows 實際 GUI／IME 驗收仍待完成，T07 未結案**，E01 不提前整合。詳見 [平行計畫](gpui-sweflash-parallel-implementation-plan-20261113.md)。

**T07／E01 正常功能進展（尚未整合）**：T07 Model／Persona 探索新增實際 Windows 假 CLI 正向測試 `717f35624`：固定唯讀命令與選項解析 **1/1 PASS**，沒有連真 Provider／帳號。E01 既有預覽→正式開檔→Quick Open Dirty Buffer 與跨 Worktree 同名路徑 **2/2 PASS**；實際修正 Quick Open **注音組字期間 Enter 不誤開檔**，以及只有 Native Editor 真正持有目標檔才關 Dialog，功能 Commit `08546db8b`，新 TDD **2/2**、Native Binary **35/35**、Windows EXE Build PASS。Windows 桌面仍有其他前景程式，未搶焦點，無實機 GUI 點擊證據；**原 NOW=T07、E01 隔離開發不提前合入中央分支**，`supported=false`。詳見 [平行計畫](gpui-sweflash-parallel-implementation-plan-20261113.md)。

**SWE-2 Flash 平行研發協議（2026-11-13）**：詳見 [兩條 worktree 子代理實作／審核與整合門檻](gpui-sweflash-parallel-implementation-plan-20261113.md)。允許 T07 與 E01 先行隔離開發，但 **NOW 仍 T07**；只按原 F1→F2→F3→F4→F5→F6 順序整合、驗收與更新 DONE。子代理無權自行合 main、Push 或發版。
