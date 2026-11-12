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
| **T03** | **New Agent** 建立真實 Provider Terminal，登入後可互動 | **部分完成**：Codex／Claude 真實 TUI、關閉重開與 Host 生命周期在隔離環境通過；六種 Provider 的 GPUI 正向點擊尚未全核實 | 由 `+` 各啟動可用 Provider、可輸入回覆；未登入的 Agent 清楚顯示原因；不用強制終止程序測試 |
| **T04** | Attach 既有 Host Session；切換／關閉分頁不終止背景 PTY | **部分完成**：真實 GUI Attach、重新開啟原 Session 已驗證；部分分頁操作仍待正常操作確認 | 左右 Pane Attach／切換／關閉／再次 Attach，分頁與 Session ID 正確 |
| **T05** | Terminal／Agent 分割、Tab 移動、右鍵、拖曳、Close Others／Right | **已有實作／待操作驗收**：雙 Pane 的操作與身份回歸已存在 | 用一個普通 Terminal、一個 Agent 及一個 Editor，完成主副窗格的右鍵、移動、分割、關閉與重新附加 |
| **T06** | Terminal 一般鍵盤、貼上／複製、Enter／Ctrl+鍵、中文輸入 | **部分完成**：真實 ASCII/TUI 輸入可用；注音候選窗及跨應用剪貼簿仍需核對 | 在真實 GUI 輸入英文、繁中，貼上／複製並確認無失焦、無重複送出 |
| **T07** | Agent Profile、Agent 自訂執行路徑、狀態錯誤提示 | **部分完成**：Host Profile、已安裝 CLI 及 Native 本地 override 均有程式入口 | 選一個有 Profile 的 Provider，確認設定→新增→輸入→關閉的完整正向流程 |

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
| **H02** | Hook 開關與通知偏好能儲存、讀回 | **部分完成**：Codex／Claude Hook CAS 及設定 UI 有接線 | 在隔離 Host 開關一次→讀回→重開設定頁一致；其他 Provider 逐項擴充 |
| **H03** | Windows 原生通知與點擊後定位到正確 Pane／Session | **部分完成**：Toast adapter 與通知去重規則已有實作；真實 GUI 通知尚未驗收 | 正常等待／完成情境看到通知，點擊定位；Classic/Native 不同時重複提醒 |

### F6｜UI／UX：最後做一致性收斂

| ID | 功能與 Classic 行為 | 初始狀態／已知證據 | 下一步與正常完成定義 |
| --- | --- | --- | --- |
| **U01** | Classic Mixed Tab／左右 Pane／`+`、右鍵與釘選 | **部分完成**：主要布局、操作模型與部分 Windows GUI 驗收已存在 | 單／雙 Pane 重複日常工作流程，控制不重疊、操作目標正確 |
| **U02** | Quick Open／檔案樹／Tab 的長檔名與資訊密度 | **部分完成**：長路徑搜尋可顯示；長 Tab 可能擁擠 | 完整檔名與路徑可辨識、列表間距合理、常用按鈕不被擠掉 |
| **U03** | 鍵盤焦點、一般 Windows 注音、快捷鍵 | **部分完成**：程式已有 IME/focus 保護，真實選字窗和 Terminal 待操作 | 編輯器、搜尋及 Terminal 正常輸入繁中，快捷鍵不搶組字、不吃大寫 |
| **U04** | 正常重開後 Workbench／Tab／草稿／釘選／設定維持 | **部分完成**：Host Session 及部分佈局重啟已有實測 | 在隔離 Workspace 關閉重開，日常佈局及資料仍正確，不需要測極端斷電 |

## 4. 確定的執行隊列（每次只讓一項成為 NOW）

| 順序 | 工作單 | 本批要交付的結果 | 狀態 |
| --- | --- | --- | --- |
| DONE | **T01** | GPUI 主／副 Pane 由 `+` 建立真正可互動 Terminal；完成一般 GUI 正向流程 | **Windows GUI + Host 讀回 PASS**；[驗收紀錄](gpui-t01-terminal-two-pane-acceptance-20261112.md) |
| DONE | **T02** | Agent 自動偵測、刷新、未安裝提示及 Classic 下拉選單操作一致 | **Windows 主／副 Pane GUI PASS + 513/513 Rust Library PASS**；[驗收紀錄](gpui-t02-agent-cli-menu-acceptance-20261112.md) |
| **NOW** | **T03** | 已安裝 Provider 的 New Agent 正常啟動、互動與錯誤提示 | 待開始／下一個工作包 |
| NEXT | T04 → T05 → T06 → T07 | Attach／Tab／Split、Terminal 鍵盤、Agent Profile | 排隊 |
| NEXT | E01 → E02 → E03 → E04 → E05 → E06 | 編輯／檔案管理／搜尋正常閉環 | 排隊 |
| NEXT | G01 → G02 → G03 → G04 → G05 → G06 | Source Control／中央 Diff／Graph／分支／Worktree | 排隊 |
| NEXT | S01 → S02 → S03 → S04 | Settings 接線、Rust 共用設定與語系 | 排隊 |
| NEXT | H01 → H02 → H03 | Hook 四態、設定、Windows 通知 | 排隊 |
| NEXT | U01 → U02 → U03 → U04 | UI／UX 功能可用性與 Classic 操作一致 | 排隊 |

**排程規則：** 該工作包完成基本正向 UI 驗收就移到下一項，不因極端案例未驗證而反覆停留；發現正常情境確實壞掉才回頭修。某項已有正常操作 PASS 可縮短，立即移到下一項。保留安全回歸，不以省時間為由刪除原有測試。

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

## 6. 既有實作／Classic 規格來源（保留歷史證據）

- **Classic 互動契約：** [gpui-flutter-classic-ux-parity-20261108.md](gpui-flutter-classic-ux-parity-20261108.md)、[gpui-flutter-interaction-parity-20261108.md](gpui-flutter-interaction-parity-20261108.md)。
- **最新常用 UI 缺口初盤：** [gpui-native-ui-ux-improvement-plan-20261110.md](gpui-native-ui-ux-improvement-plan-20261110.md)；早期的 PLAN／FAIL 是該版歷史，不直接代表今天仍失敗。
- **Terminal／Agent：** [gpui-t01-terminal-two-pane-acceptance-20261112.md](gpui-t01-terminal-two-pane-acceptance-20261112.md)（真實 GUI `New Terminal` 雙 Pane 驗收）；[gpui-t02-agent-cli-menu-acceptance-20261112.md](gpui-t02-agent-cli-menu-acceptance-20261112.md)（本次 Agent 偵測、空清單、刷新與 Classic 名稱對齊）；[gpui-agent-cli-dropdown-parity-20261112.md](gpui-agent-cli-dropdown-parity-20261112.md)（含 real Codex／Claude、主副 Pane、Host Session）；[gpui-terminal-classic-transport-parity-20261109.md](gpui-terminal-classic-transport-parity-20261109.md)。
- **Git 與中央 Diff：** [gpui-central-diff-branch-audit-20261112.md](gpui-central-diff-branch-audit-20261112.md)、[gpui-source-control-git-graph-parity-20261109.md](gpui-source-control-git-graph-parity-20261109.md)。
- **設定／Hook：** [gpui-agent-hook-status-notification-parity-20261111.md](gpui-agent-hook-status-notification-parity-20261111.md)、[gpui-dart-component-reuse-tdd-20261109.md](gpui-dart-component-reuse-tdd-20261109.md)。
- **其他舊里程碑：** [gpui-p0-p1-p2-tdd-closure-20261108.md](gpui-p0-p1-p2-tdd-closure-20261108.md)、[gpui-active-work-ledger-20261107.md](gpui-active-work-ledger-20261107.md)。
- **最後一次基線回歸（來自上輪、非本次重跑）：** GPUI Library **512/512 PASS**、Native Binary **31/31 PASS**；隔離 Host 建立請求跨重啟模擬 **1/1 PASS**，提交 `789f2fa00`。此數字不取代後續功能 UI 實測。
