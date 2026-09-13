# Workbench Sidebar:自動封存、狀態彙總、跨專案 Parent、Commit Graph - 問題確認、設計與實作紀錄

日期:2026-09-12。範圍:`lib/src/features/workbench/`、`rust/alera-core`、`rust/alera-cli`、`rust/`(FRB)、`mobile/`。文件前半是確認與設計,四個議題已全部實作並合併,實作結果與決策採用值見文末「實作結果」。

## 實作結果(2026-09-12 合併)

四個議題各一個 branch,依序合併進 `refactor/architecture-guard-ci`,合併皆無衝突:

| # | Branch / Commit | 採用設計 |
|---|---|---|
| 3 | `fix/workspace-parent-same-project` (`9b5a24e4`) | 方向 A:三個 parent 入口改為同專案過濾;既有跨專案 link 保留可檢視與清除,dialog 標示 `(other project)`;Dart service 與 Rust `link_workspaces` 都拒絕新跨專案 link。 |
| 2 | `feat/sidebar-agent-counts` (`fe6d7314`) | 左側 14px slot 不動(仍顯示最急迫狀態 glyph);`WorkbenchTabCompletionAcknowledgements` 提升為 keepAlive provider(`workbench_tab_acknowledgements.dart`),`groupWorkspaceAgentRuns` 增加 `doneUnacked` 桶;右 tray `WorkspaceAgentCompactSummary` 顯示 per-group glyph + count;project header 右側加彙總 badges;ack 維持 session-local。 |
| 1 | `feat/workspace-archive` (`79ea3c49`) | `archivedAt: DateTime?` 旗標端到端(Dart domain + mapper、Rust model + store schema 遷移、runtime client JSON、Drift table);`archiveWorkspace` 先 sleep 再蓋 `archivedAt`,`restoreWorkspace` 清旗標並可選 select;`WorkbenchArchivedHeaderRow` 依 groupBy 分組(每專案下可收合 Archived 子區塊、section 模式全域群組、none 模式 header);`workbenchArchiveSweepProvider` 啟動跑一次 + 每小時掃;`GeneralSettings.autoArchiveWorkspacesAfterDays` 預設 30、0 關閉;封存列不計入主計數但搜尋找得到;mobile 讀 `archivedAt` 過濾。 |
| 4 | `feat/commit-graph-mainview` (`bb6c460f`) | 新增 `WorkspaceTabKind.gitHistory` 主區 tab(dedup、持久化、panel header 放大入口);`git_history` FRB 簽名加 `includeAllRefs`(walk 全部 `refs/heads/*` 與 `refs/remotes/*` tip,排除 `*/HEAD`)與 `offset` 分頁;`buildGitHistoryViewModelsFromItems` 加 `initialSwimlanes` 讓分頁接續 lanes;新共用檔 `workspace_git_history_graph.dart` 與主區 `workspace_git_history_surface.dart`(All Branches toggle 預設開、refresh、滾動到底自動載入);panel 維持 current-branch 行為;mobile 對新 kind 有 fallback。 |

決策採用值對照原「待決策問題」:1 全域 30 天可關;2 封存前 sleep、sweep 只挑無 tab 且無 running agent 的;3 搜尋找得到、不計入主計數;4 採 `archivedAt` 旗標;5 ack 維持 session-local;6 左 slot 不動、計數在右 tray;7 project header 彙總全部狀態桶、封存排除;8 方向 A、既有 link 標示可清除;9 mobile 先過濾不顯示封存區;10 主區 tab、panel 保留;11 含 remote-tracking、主區預設開、狀態放 tab payload;12 採 `offset` 分頁;13 維持 `commit_touches_workspace` 過濾。

## 結論摘要

| # | 議題 | 確認結果 |
|---|---|---|
| 1 | 自動封存長時間沒異動的 worktree,預設分類到各專案下的封存區,可恢復 | 目前無任何封存概念。需要新增 persisted 狀態旗標、sidebar 分組、以及一個定期掃描入口。最大風險是 worktree reconcile 會把「磁碟上還在」的封存 worktree 重新註冊回來,設計上必須避開。 |
| 2 | Sidebar 每個 worktree 前面的綠點改成更多資訊(執行中數、已完成待讀數),專案列右邊顯示底下彙總 | 可行但缺一塊拼圖:「已完成待讀」的已讀狀態(`WorkbenchTabCompletionAcknowledgements`)目前是 shell page 的 session-local 物件,sidebar 的純函數 row builder 拿不到,需要先提升成 provider。 |
| 3 | 父工作區下拉選單可以跨專案選,疑似有問題 | 確認為半缺陷:三個選取入口都允許跨專案、Rust 後端也不擋,但 sidebar 樹只會在同專案(或同 section)的 sibling group 內嵌套,跨專案 link 在樹上永遠看不到,卻仍影響 pin-tree 與 cycle check。 |
| 4 | Commit graph 可放大到主畫面,並顯示完整分支 | 新功能。目前 graph 只存在 Source Control 右側欄底部的小區塊,無放大入口;且 Rust 端 revwalk 只從 HEAD + upstream 起走,其他 local/remote 分支的 commit 不會出現,所以「完整分支」需要改 `git_history` 的 walk 範圍。 |

## 1. 自動封存長時間沒異動的 worktree

### 現況

- `Workspace` 模型(`lib/src/features/workbench/domain/workspace.dart`)有 `status: WorkspaceStatus { active, removed }`、`createdAt`、`updatedAt`,沒有任何封存欄位。Rust 端 `rust/alera-core/src/runtime/models.rs:42-67`(struct)與 `92-113`(`WorkspaceStatus` enum)同樣只有 `Active`/`Removed`。
- 列表永遠只回傳 active:Drift `listWorkspaces`/`watchWorkspaces` 用 `status = 'active'` 過濾(`lib/src/features/workbench/infra/drift_workbench_repository.dart:13-43`);runtime host 的 `list_workspaces`/`list_all_workspaces` SQL 同樣是 `status = 'active'`(`rust/alera-core/src/runtime/store.rs:795-818`)。`removed` 目前實務上是 hard-delete,`removeWorkspace` 直接刪 row。
- 已有「Sleep」(`workspace.sleep`,Dart 側 `WorkbenchController.sleepWorkspace` 在 `workbench_controller_projects.dart:75-99`):關閉 tabs、layout、terminal sessions,worktree 留在列表。Sleep 是「關 session」,不是「移出清單」,兩者是不同維度。
- `WorkspaceSection` 是全域的,沒有 `projectId`(domain `workspace_section.dart`;Rust `workspaceSections` table,`workspace_section_store.rs:18-26`)。所以「封存區」不能直接用現有 section 機制達成「各專案下」的需求,需要 listing 層自己分組。
- 活動訊號已存在:`WorkspaceActivityController`(`workspace_activity_controller.dart`)記錄每個 workspace 的最近活動時間(agent 狀態轉換、terminal exit),經 `workspaceActivity.*` 持久化到 host;`workspace.updatedAt` 在 rename、switch branch、reconcile 修正時更新。從未開過 terminal 的 workspace 沒有 activity record,fallback 要用 `updatedAt`/`createdAt`。
- 「已讀」概念已存在但僅限 tab strip:`WorkbenchTabCompletionAcknowledgements`(`workbench_tab_attention.dart`)由 `_AleraShellPageBodyState` 持有(`alera_shell_page_body.dart:5-6`),只給 tab chip 用。

### 關鍵風險:reconcile 會復活封存 worktree

`WorkspaceServiceReconciliation.reconcile`(`workspace_service_reconciliation.dart:51-80`)會把 `git worktree list` 回報、但不在 `listWorkspaces` 結果裡的每個磁碟上 worktree 註冊為**新的 active workspace**。封存只改列表可見性、不刪 worktree,所以:

- 若封存用「不列入 active 列表」的方式實作(例如新 `WorkspaceStatus.archived`),reconcile 每 60 秒由 `GitWorktreeMetadataWatcher` 輪詢觸發、外加手動 Refresh Worktrees,都會把封存中的 worktree 重新加成 active。除非 reconcile 的 tracked-path 集合改成用 `status != 'removed'` 的全部 row 來比對。
- 若封存用「保留 `status='active'` + 新增 `archivedAt` 旗標」,reconcile 天然安全:封存的 row 仍在 `listWorkspaces` 結果中,tracked path 涵蓋它,不會重複註冊。代價是所有「假設 listWorkspaces 回傳的都是工作中的 workspace」的消費端都要加 `isArchived` 判斷。

### 設計建議

**資料模型**:採用 `archivedAt: DateTime?`(UTC)欄位,而不是新 enum 值。理由:

- `status` 維持生命週期語義(active/removed),`archivedAt` 表達展示狀態,兩者正交。
- `workspace.upsert` 已是全列 value write,加一個 `#[serde(default)] archived_at: Option<DateTime<Utc>>` 欄位即可讓封存/還原走既有寫入路徑;是否要加專用 verb(`workspace.setArchived`)是選配,加了好處是 mobile allowlist 與語意明確。
- 需要同步的地方:`rust/alera-core/src/runtime/models.rs` 的 `Workspace`、`store.rs` 的 SELECT 欄位與 upsert、workspaces table 加 column(schema 為 `CREATE TABLE IF NOT EXISTS` 風格,要補 `ALTER TABLE ... ADD COLUMN` 的 migrate 判斷)、Dart `workspace.dart` + mapper、`runtime_workbench_repository.dart` 與 `runtime_managed_workspace_client.dart` 的 `_workspaceFromJson`/`_workspaceToJson`、Drift `workspacesTable` + `drift_workbench_repository.dart`(若 Drift 路徑仍是 supported fallback)。

**封存區呈現**(「封存清單還是放在各專案下」):

- `groupBy: project`:每個 project header 下、一般 workspace 列之後,加一個可收合的 `Archived` 子區塊(新 row type,例如 `WorkbenchArchivedHeaderRow`,帶 count;收合狀態存 `viewPrefs.collapsedArchivedProjectIds` 或一個全域 flag)。列用 muted 樣式,context menu 提供 `Restore` 與 `Remove`。
- `groupBy: section`:section 是全域的,封存 workspace 放在所有 section 之後的獨立 `Archived` 群組,不跟隨原 sectionId(封存時不需清 sectionId,還原後回原 section)。
- `groupBy: none`:與 `All` header 同模式,加 `Archived` header。
- Pinned 區塊不含封存 row;`isPinned` 與封存互斥或在封存時忽略 pin 顯示(建議封存不自動拔 pin,但 pinned row 不出現在 pinned 區)。

**自動封存掃描**:

- 新增一個 keepAlive coordinator provider(比照 `workspaceActivityCoordinator` 的掛法,在 `alera_shell_page_body.dart` watch),bootstrap 後跑第一次,之後定期(例如每小時)掃。
- 判定條件:`workspace.isMain == false`、非 pinned、`!workspace.isArchived`、`state.tabsFor(id)` 無任何 tab(或只在 sleep 狀態)、無 non-done agent run、最後活動時間 `max(activityMap[id], updatedAt, createdAt)` 超過門檻。
- 門檻放 `GeneralSettings`(比照 `confirmProjectRemoval` 的寫法,`alera_settings.dart` + `runtime_settings_repository.dart`):例如 `autoArchiveWorkspacesAfterDays`,0 或 null 表示關閉。預設值要決定(見待決策)。
- 封存前是否先 sleep:建議**是**。封存語意是「不在使用中」,順便關掉殘留 tabs/terminal sessions 才一致;流程可重用 `WorkbenchSleepWorkspaceCoordinator` 再寫 `archivedAt`。要留意 sleep 對 dirty editor 的警告流程(目前是 dialog,自動路徑不能彈 dialog,自動封存應只挑無 tab 或已無 session 的,或先記錄再略過 dirty 的)。
- 還原:點擊封存列或 `Restore` → 清 `archivedAt`、寫入 `updatedAt`、回到一般清單;若有需要再由 selection hydrator 建初始 terminal。

**列表消費端要處理 `isArchived` 的位置**(選 A 方案時):

- `buildSidebarRows` / `_WorkbenchSidebarRowBuilder`:分流出封存群組。
- 三個 parent 選取來源(parent candidates):排除已封存。
- `workspaceMatchesActiveFilter`、kind/tag filter:封存 row 是否參與搜尋與過濾要定義(建議搜尋仍找得到,方便「以後使用」)。
- `visibleSidebarCollapseTargets`、`countVisibleWorkspaces`、collapsed rail 的 workspace count:決定封存是否計入(建議不計入主數字,Archived header 自己帶 count)。
- mobile:`workspace.listAll`、`workspaceSidebar.snapshot` 會開始回傳帶 `archivedAt` 的 row;mobile 要過濾或分組,否則封存 workspace 會出現在手機清單。需要新 capability(例如 `workspaceArchiveV1`)或 mobile 端直接讀欄位自行過濾。

## 2. Sidebar 狀態點改為更多資訊

### 現況

- 左側固定 14px slot:`AgentRunStateIndicator`(`widgets/agent_run_state_indicator.dart`),顯示 `mostUrgentWorkspaceAgentRun(agentRuns)` 的單一狀態(working=amber spinner、waiting=amber bell、blocked=red bell、done=green check、interrupted=red cancel);無 status 時顯示 `AleraStatusDot`(active 或 hasTerminalTabs 時綠色、否則灰),這就是使用者說的「綠點」。固定寬度是刻意的:註解(`project_workbench_workspace_rows.dart:126-137`)說明換 widget type 會重建 element、重啟 spinner。
- 右側 tray 已經存在:`WorkspaceAgentCompactSummary`(`widgets/workspace_agent_compact_summary.dart`)在有 agent run 時顯示分組 glyph + 最多 3 個 agent identity icon + `+N` 溢出,點擊展開 agent 列。也就是「更多資訊」有一半已經在右邊。
- Row 資料已在 row builder 算好:`_appendWorkspaceTreeRows`(`workbench_sidebar_row_builder.dart:247-280`)產生 `agentRuns` 與 `agentRunGroups`(`groupWorkspaceAgentRuns` 分成 waiting/blocked/interrupted/working/done 五桶)。
- `pendingReviewAgentCount`(`workspace_agent_status_projection.dart:93-101`)已存在且被 tray badge/dock badge 重用(`desktop_presence.dart`)。
- 「已完成待讀」=`done` 且未被 `WorkbenchTabCompletionAcknowledgements` ack;目前 ack map 在 shell page state 內,sidebar 的 `buildSidebarRows` 是純函數、只收 `agentStatuses` 與 `lastActivityByWorkspaceId`,拿不到 ack 狀態。
- Project header(`_ProjectHeaderTile`,`project_workbench_sidebar_body.dart:307-433`)右邊目前只有 `workspaceCount` 數字 + chevron。

### 設計建議

- **左側 slot**:使用者要的是把單一綠點換成有資訊量的指示。建議保持左 slot 固定寬度、仍然顯示「最急迫狀態」glyph(at-a-glance 語義不變),把數字資訊放右側;若要照字面把計數放左邊,需要把 slot 改成固定兩格寬或允許不等寬導致名稱水平跳動,取捨寫在待決策。
- **右側 tray 增強**:`WorkspaceAgentCompactSummary` 目前只顯示 glyph + identity icon。可改成每個 group 顯示 `glyph + count`(例如 spinner+2、bell+1、check+3),空間不夠時沿用 `+N` 折疊。需要的只是 `WorkspaceAgentRunGroup.runs.length`。
- **「待讀」桶**:把 `groupWorkspaceAgentRuns` 的 `done` 桶再細分 `doneUnacked`(status.state == done 且未被 ack)。需要先將 acknowledgements 提升為可注入的 provider(見下)。
- **ack 狀態提升**:把 `WorkbenchTabCompletionAcknowledgements` 從 `_AleraShellPageBodyState` 的 field 改成 keepAlive Riverpod provider(或 notifier 包一層 immutable map),shell page 與 `workbenchSidebarRows` provider 都讀它;`buildSidebarRows` 多收一個 `isDoneAcknowledged(terminalSessionId, stateStartedAt)` 或 map 參數。注意 ack 是 session-local、不持久化,重開 app 後所有恢復的 done status 都會是 unacked,這是不是想要的行为要確認(見待決策)。
- **專案彙總**:`WorkbenchProjectHeaderRow` 新增彙總欄位(例如 `runningCount`、`attentionCount`、`doneUnackedCount`),在 `_appendProjectGroups` 內對該 project 的 visible workspaces 的 `agentRunGroups` 加總;`_ProjectHeaderTile` 右側在 count 前渲染 compact badges(與 workspace row 同一套 glyph+count 元件)。封存的 workspace 建議不計入。
- section header(`_WorkspaceSectionHeader`/`_SidebarSectionTile`)、Pinned/All header、collapsed rail 是否要同樣彙總,範圍問題,建議先做 project header,其餘待決策。

## 3. 跨專案 Parent 確認

### 現況(確認可跨專案)

三個入口的 parent 候選都來自「所有專案的所有 active workspace」:

- Sidebar context menu `Set Parent Workspace` → `_workspaceParentOptions()`(`project_workbench_sidebar_actions.dart:294-302`)直接遍歷 `state.projects` × `workspacesFor`,沒有 projectId 過濾;dialog(`workspace_graph_dialogs.dart:373-396`)每個 option 顯示 `project / workspace - branch`(label 定義在 `workspace_graph_dialogs.dart:23-27`)。
- `PromptWorkspaceDialog` 的 Parent Workspace 下拉(`prompt_workspace_dialog_form.dart:69-89`)吃 `widget.parentWorkspaces`,由 `showCreateWorkspaceFlow`(`workbench_dialog_launchers.dart:225-230`)跨全部 `state.projects` 組成。
- `CreateWorkspaceDialog` settings step(`create_workspace_dialog_settings_step.dart:67-85`)吃同一批 `parentCandidates`。
- 排序 `compareWorkspaceParentSelectionKeys`(`workspace_parent_selection_order.dart`)有 `preferredProjectId`,只把本專案排前面,不過濾。
- 後端 `RuntimeStore::link_workspaces`(`rust/alera-core/src/runtime/store.rs:1220-1291`)只檢查 self-link、雙方存在、cycle、已有 parent,**不檢查同專案**。

### 為什麼是半缺陷

- `buildWorkspaceTree`(`workbench_listing_tree.dart:37-49`)只在「parent 也存在於同一個 sibling group 的 `entries`」時才把 child 掛到 parent 下。`groupBy: project` 時 entries 是單一專案的 workspaces,跨專案 parent 不在其中,child 被提升到該專案 root。樹上永遠看不到這層關係。
- 但這層關係仍然真實存在且有副作用:`_workspaceHasDescendants`(`project_workbench_sidebar_body.dart:179-183`)掃的是 `state.workspacesByProject` 全部專案,所以跨專案 child 會讓另一個專案的 parent 出現 `Pin Workspace Tree` 選項,而且 `setWorkspaceTreePinned` 的 descendant 掃描(`workspace_descendants.dart`)也是全域的,會把別的專案的 workspace 一起 pin。child 端則出現 `Clear Parent Workspace`。
- mobile 端已經把跨專案 parent 當成邊界情況處理:mobile AGENTS.md 明定「selected parent 屬於其他專案時 MUST skip 不做 sibling worktree 索引」,等於承認這種 link 會存在但語意上無法參與。

### 建議

- **方向 A(建議)**:限制 parent 只能同專案。三處候選清單都加 `candidate.workspace.projectId == workspace.projectId`(dialog)或 `== _selectedProject?.id`(create/prompt dialog);`WorkbenchWorkspaceParentUpdateService.update` 加同專案檢查,讓 UI 之外的呼叫路徑也被擋;host `link_workspaces` 可選擇性加同專案驗證(會影響既有 cross-project link 的修法,見下)。
- **方向 B**:保留跨專案 link 但補齊語意,例如 section/flat 模式下可嵌套、`groupBy: project` 下在 child row 顯示「linked to 其他專案」的指示。成本高、價值不明。
- **既有跨專案 link 的處理**:不論選 A/B,已存在的 link 不該默默失效。建議保留渲染(維持現狀:child 在 root、可 Clear Parent),Set Parent dialog 對非本專案的既有 parent 顯示但標記;或者在 migration 時一次性清除跨專案 relation 並記 log。要決策。
- 附帶觀察:`WorkspaceParentOption.label` 已含 project 名稱,使用者看得到來源專案,所以這比較像「功能開放過頭」而非「顯示錯誤」。

## 4. Commit Graph 放大到主畫面與完整分支

### 現況

- Commit graph 是 `_GitHistoryPanel`,掛在 Source Control 頁籤(`workspace_context_sidebar.dart:83-106` 的 `WorkspaceGitDiffPanel`,`workspace_git_diff_panel.dart:260-272`)的最底部。它是右側欄內的一個可收合、可拖曳高度的區塊:預設高 256,範圍 96-520(`workspace_git_history_panel.dart:49-51`),寬度受右側欄寬度限制。**沒有任何放大/移到主畫面的入口**。
- 資料來源是 FRB `rust.gitHistory(path, limit, baseRef)`(`rust_git_backend.dart:252-263` → `rust/src/api/git_history_impl.rs`)。目前唯一的呼叫點是 `workspace_git_diff_panel.dart:859`,固定 `limit: 50`、不傳 `baseRef`。
- Rust walk 範圍(`git_history_impl.rs:62-69`):revwalk 只 `push(head_oid)` 和 upstream 的 `remote_oid`,`Sort::TOPOLOGICAL | Sort::TIME`。其他 local branch(`refs/heads/*`)與 remote head **不會被 walk**,除非它們是 HEAD 的祖先。這就是「看不到完整分支」的直接原因。
- `refs_by_oid`(`git_history_impl.rs:215-257`)蒐集所有 local/remote branch 與 tag,但只用來替「已在 walk 結果內的 commit」貼 ref badge,不影響 walk 範圍。
- Workspace 子目錄 scope:`commit_touches_workspace`(`git_history_impl.rs:259-294`)在非 root scope 時只保留動到該路徑的 commit,`rewrite_history_parents_to_visible_ancestors`(`321-342`)把 parent 重接到最近的可見祖先。
- 分頁:`DEFAULT_HISTORY_LIMIT = 50`、`MAX_HISTORY_LIMIT = 200`(`git_history_impl.rs:12-13`),`hasMore` 有回傳,但 UI 只在 header 顯示 `count+`(`workspace_git_history_panel.dart:345-353`),沒有 load more 機制,也沒有 cursor 參數(只有 `limit`)。
- 圖形渲染:每列一個 `CustomPaint`(`_GitHistoryGraphPainter`,`workspace_git_history_panel_graph.dart`),lane 寬 11、列高 24;`buildGitHistoryViewModelsFromItems`(`git_history_graph.dart:68-189`)算 input/output swimlanes,顏色在 5 個 lane color 上輪轉(`git_history_graph_models.dart`),另有 ref/remote/base 專用色;Dart 側另外插入 Incoming/Outgoing Changes 合成列(`git_history_graph.dart:269-290`)。
- 列互動:點列展開 commit 檔案清單、`onOpenCommit` 開 `gitDiff` 主區 tab、複製 hash/message、local branch badge 右鍵 Switch to Branch(`workspace_git_history_panel.dart:237-317`、`workspace_git_history_panel_row.dart`)。
- 主區 tab 系統:`WorkspaceTabKind` 目前有 `terminal`/`editor`/`markdownViewer`/`pdf`/`gitDiff`(`workspace_tab_record.dart:7-12`),`_WorkspaceTabContent` 依 kind 分派(`workspace_workbench_tab_content.dart:42-67`);開 gitDiff tab 走 `WorkspaceTabGitOpening.openOrCreateGitCommitDiffTab`(`workspace_tab_git_opening.dart:62-128`),已有 per-workspace 持久化與 dedup 的完整先例。

### 設計建議

**放大到主畫面**:新增 `WorkspaceTabKind.gitHistory` 主區 tab(比照 gitDiff tab 的做法,而不是 dialog)。

- `workspace_tab_record.dart` 加 enum 值 + payload key(scope 的 `gitDiffRoot` 可重用);`isFilePreviewSlot` 的 switch 要補 case(建議 `false`,它不是 file preview)。
- `WorkspaceTabService` 加 `openOrCreateGitHistoryTab`(dedup:同 workspace + 同 root 重用既有 tab);`_WorkspaceTabContent` 的 switch 加 case。
- 入口:`_GitHistoryPanel` header 在 refresh 按鈕旁加一個「放大」icon button(title case 文案,例如 `Open Commit Graph`),開啟 tab 並保持 panel 原樣。
- 主區視圖是新的寬版 layout,不是把 panel 塞進去:左側固定寬度的 graph column(可水平捲動,因為 lane 數會隨分支增加),右側欄位化 subject、ref badges、author、relative time。`_GitHistoryCommitRow`/`_GitRefBadge` 目前是 `workspace_git_diff_panel.dart` 的 part-private 類別,需要拆到共用檔或寫主區專用 row。
- 點 commit 仍可重用 `onOpenGitCommitDiff` 開 `gitDiff` tab;檔案清單展開可沿用 `commitCompare` 流程。

**顯示完整分支**:`git_history` 加 walk 範圍參數。

- FRB 簽名加參數,例如 `walkScope: current | all` 或 `includeAllRefs: bool`;`all` 時把所有 `refs/heads/*` tip 推進 revwalk(是否含 `refs/remotes/*` 見待決策),讓每條分支 tip 都出現在圖上。`refs_by_oid` 已經會替這些 tip 貼 badge,lane 產生器本來就支援任意 lane 數。
- 簽名變更要 `make frb-generate` 重新產生 `lib/src/rust/` bindings 並連同提交;`GitBackend.history`(`git_backend.dart:161`)、`RustGitBackend`、`FakeGitBackend`(`test/unit/fake_git_backend.dart`)同步更新。
- Incoming/Outgoing 合成列維持只跟 current+upstream 掛鉤,不受 all-branches 影響。
- 子目錄 scope 下 all-branches 仍套用 `commit_touches_workspace` 過濾(建議維持,待確認)。
- 分頁:全分支圖更需要 load more。目前只有 `limit`(上限 200),沒有 cursor;建議加 `beforeOid`/`startOid` cursor 參數,或先把 limit 提高。主區視圖滾到底時載入下一頁並把新 viewModels 接在既有 swimlane 狀態後面(`buildGitHistoryViewModelsFromItems` 需支援接續初始 lanes,或分段各自算但要保證 lane 對齊)。
- UI:panel header 與主區 toolbar 各放一個「All Branches / Current Branch」toggle;預設值與是否 persist(進 `viewPrefs` 或 tab payload)見待決策。
- 效能注意:lane 顏色只有 5 個會快速重複,可接受;lane 過多時 graph column 需水平捲動或上限折疊。

## 待決策問題

1. 封存門檻預設值與設定位置:`GeneralSettings` 全域一個天數?是否要 per-project override?預設關閉還是預設 30 天?
2. 自動封存要不要先強制 sleep?有 dirty editor / 還在跑的 session 時略過還是記下來提示?
3. 封存 workspace 是否算進 sidebar 搜尋結果與 kind/tag 過濾?(建議:搜尋找得到、不算入主計數。)
4. `archivedAt` 旗標(建議)vs `WorkspaceStatus.archived`:文件採前者,若要改用 status,需要同時修 `list_workspaces` SQL、reconcile 的 tracked set、以及所有 `isActive` 消費端,範圍更大。
5. 「已完成待讀」的 ack 是否持久化?目前 session-local,重開 app 即歸零;若要在 sidebar 顯示「待讀 N」,重開後會全部變回待讀,可能不是期望。
6. 左側 slot:維持固定 14px + 右 tray 加計數(建議),還是接受不等寬的計數 chip 直接取代綠點?
7. Project header 彙總要顯示哪些桶(working / waiting+blocked / doneUnacked / interrupted)?封存 workspace 排除?
8. 跨專案 parent:採方向 A(限同專案)或 B;既有跨專案 link 的 migration 策略。
9. mobile 是否同步顯示封存區,或先只在 desktop 做、mobile 端先過濾掉?
10. Commit graph 放大形式:新增 `WorkspaceTabKind.gitHistory` 主區 tab(建議)還是最大化 dialog?panel 內是否保留原小圖?
11. 完整分支範圍:walk 所有 `refs/heads/*`?要不要含 `refs/remotes/*`?預設開或關?toggle 狀態放 `viewPrefs` 還是 tab payload?
12. 分頁策略:加 cursor(`beforeOid`)還是單純調高 limit 上限?`MAX_HISTORY_LIMIT = 200` 是否足夠?
13. 子目錄 scope 的 workspace 開 all-branches 時,是否仍只顯示動到該路徑的 commit(建議維持 `commit_touches_workspace` 過濾)?

## 任務拆解(已實作,對應見上方實作結果表)

1. `Workspace` 加 `archivedAt`(Dart domain + mapper、Rust model + store schema/SELECT/upsert、兩個 runtime client 的 JSON 映射、Drift table + repository)。依賴:決策 4。
2. Sidebar listing:`WorkbenchArchivedHeaderRow` + 各 groupBy 的封存群組拆分 + `viewPrefs` 收合狀態。依賴:1。
3. Workspace context menu 加 `Archive`/`Restore`;`_setWorkspaceArchived` 走 `workspace.upsert` 或新 verb;封存時先 sleep。依賴:1、決策 2。
4. 自動掃描 coordinator + `GeneralSettings.autoArchiveWorkspacesAfterDays` + application pane 設定項。依賴:1、決策 1、2。
5. `WorkbenchTabCompletionAcknowledgements` 提升為 provider;`groupWorkspaceAgentRuns` 分 `doneUnacked`;`WorkspaceAgentCompactSummary` 顯示 per-group count;`WorkbenchProjectHeaderRow`/`_ProjectHeaderTile` 加彙總。依賴:決策 5、6、7。
6. Parent 候選加同專案過濾(三處)+ service 層同專案檢查 + 存量 link 策略。依賴:決策 8。
7. `WorkspaceTabKind.gitHistory` + payload + `openOrCreateGitHistoryTab` + `_WorkspaceTabContent` case + panel header 放大入口。依賴:決策 10。
8. `git_history` 加 walk-scope 參數(FRB signature、regen bindings、`GitBackend`/`RustGitBackend`/`FakeGitBackend`)+ panel 與主區的 branch toggle。依賴:決策 11、12、13。
9. 主區寬版 graph view:拆出/重用 row 與 painter、graph column 水平捲動、load-more 分頁接上既有 lanes。依賴:7、8。

## 驗證方式

- `test/unit` 已有 `workbench_listing`/`buildWorkspaceTree`/section 相關測試與 `workbench_sections_recovery_test.dart`;封存分組、doneUnacked 分桶、project 彙總、parent 同專案過濾都可用純函數測試覆蓋。
- Rust 側 `store.rs` 的 `archivedAt` schema/serde 走 `alera-core` 既有 store tests;`workspace_contract_tests.rs` 的 contract matrix 要補列。
- Commit graph:`test/unit/git_history_graph_test.dart` 擴充 multi-head/all-branches walk 的 lane 斷言;widget 測試(`workspace_git_history_panel_test.dart` 模式)覆蓋放大入口與 branch toggle;`FakeGitBackend` 補 walkScope 參數記錄;Rust 端 `git_history_impl.rs` 加 revwalk 單元測試。FRB 簽名變更後跑 `make frb-generate` 並確認 bindings 一致。
- 手動驗收:封存後 Refresh Worktrees 與 60 秒 watcher 不得把封存 worktree 重新加回;restore 後回到原 project/section;跨專案 parent 選項消失且既有跨專案 link 行為符合決策 8;commit graph 在主區 tab 顯示全部分支 tip badge 與 lanes,load more 後 lanes 對齊不跳動。

## 假設

- 「長時間沒異動」以 app 內活動訊號(agent 狀態轉換、terminal exit、`updatedAt`)為準;不含 git 層面的新 commit/dirty 偵測(那是額外 probe,成本更高,先不做)。
- 封存不動磁碟上的 worktree、不刪 branch,只改列表歸屬;Remove 仍是既有的破壞性流程。
- 封存狀態存在 host(runtime store)而非純 local view pref,讓 mobile/其他 client 能看到一致狀態。
