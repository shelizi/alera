part of 'alera_localizations.dart';

// Text shown in Git History / Git Graph right-click menus, confirmation
// dialogs, and action feedback. Values inside {placeholders} are supplied at
// runtime after translation, so ref names, subjects, and paths stay verbatim.

const Map<String, String> _traditionalChineseGitHistory = <String, String>{
  'Compare with Branch...': '與其他分支比較…',
  'Rename Branch': '重新命名分支',
  'Delete Branch?': '要刪除此分支嗎？',
  'Force Delete Branch?': '要強制刪除此分支嗎？',
  'Delete Tag?': '要刪除 Tag 嗎？',
  'Discard All Changes?': '要捨棄所有變更嗎？',
  'Delete Forcefully': '強制刪除',
  'Checkout': '簽出',
  'Revert': '還原',
  'Commit Hash': 'Commit 雜湊值',
  'Commit Subject': 'Commit 主旨',
  'This deletes the remote branch and cannot be undone.': '這會刪除遠端分支，且無法復原。',
  'This creates a merge or fast-forward on the current branch.':
      '這會在目前分支建立合併 Commit，或執行快轉合併。',
  'This permanently discards all staged, unstaged and untracked changes in this workspace.':
      '這會永久捨棄此工作區所有已暫存、未暫存及未追蹤的變更。',
  'Stashed changes': '已暫存變更至 Stash',
  'Discarded all changes': '已捨棄所有變更',
  'Cherry pick was aborted because of conflicts.': 'Cherry-pick 因衝突而中止。',
  'Drop was aborted because of conflicts.': '移除 Commit 因衝突而中止。',
  'Merge stopped with conflicts. Resolve conflicts in Source Control.':
      '合併遇到衝突，請在原始碼管理中解決。',
  'Rebase was aborted because of conflicts.': 'Rebase 因衝突而中止。',
  'Merge conflicts occurred. Resolve conflicts in Source Control.':
      '合併發生衝突，請在原始碼管理中解決。',
  'Rebase conflicts were aborted. Resolve conflicts in Source Control.':
      'Rebase 因衝突而中止，請在原始碼管理中確認並解決。',
  'This workspace is not a Git repository.': '此工作區不是 Git Repository。',
  'Cannot run this action from detached HEAD.': '無法在 detached HEAD 狀態執行此操作。',
  'Remote origin was not found.': '找不到 origin 遠端。',
  'Nothing to commit.': '沒有可提交的變更。',
  'Resolve conflicts before continuing.': '請先解決衝突再繼續。',
  'Git operation failed.': 'Git 操作失敗。',
  'Could not create a worktree: project not found': '無法建立 Worktree：找不到專案。',
  'Copied branch name: {branch}': '已複製分支名稱：{branch}',
  'Renamed {old} to {new}': '已將 {old} 重新命名為 {new}',
  'Merged {branch}': '已合併 {branch}',
  'Rebased onto {branch}': '已 Rebase 到 {branch}',
  'Created archive {path}': '已建立封存檔 {path}',
  'Pulled {branch}': '已 Pull {branch}',
  'Pushed tag {tag}': '已 Push Tag {tag}',
  'Deleted tag {tag}': '已刪除 Tag {tag}',
  'Deleted branch {branch}': '已刪除分支 {branch}',
  'Could not parse remote branch {branch}': '無法解析遠端分支 {branch}',
  'Deleted {branch} on {remote}': '已從 {remote} 刪除 {branch}',
  'Checked out {name}': '已簽出 {name}',
  'Added tag {tag}': '已新增 Tag {tag}',
  'Created branch {branch} at {commit}': '已於 {commit} 建立分支 {branch}',
  'Cherry-picked {commit}': '已 Cherry-pick {commit}',
  'Dropped {commit}': '已移除 Commit {commit}',
  'Current branch fast-forwarded to {commit}': '目前分支已快轉至 {commit}',
  'Merge commit created from {commit}': '已從 {commit} 建立合併 Commit',
  'Rebased {branch} onto {commit}': '已將 {branch} Rebase 到 {commit}',
  'Created archive at {path}': '已在 {path} 建立封存檔',
  'Reverted {commit}': '已還原 {commit} 的變更',
  'Reset to {commit}': '已重設至 {commit}',
  'Created workspace {branch}': '已建立工作區 {branch}',
  'Could not switch to {branch}: project not found': '無法切換至 {branch}：找不到專案。',
  'Switched to {branch}': '已切換至 {branch}',
  '{label} copied': '已複製{label}',
  'Could not copy {label}': '無法複製{label}',
  'Delete the local branch "{branch}"?': '要刪除本機分支「{branch}」嗎？',
  'Branch "{branch}" was not fully merged. Force deletion removes the branch reference and cannot be undone.':
      '分支「{branch}」尚未完全合併。強制刪除會移除分支參照，且無法復原。',
  'Delete {branch} on {remote}?': '要從 {remote} 刪除分支 {branch} 嗎？',
  'Delete the local tag "{tag}"?': '要刪除本機 Tag「{tag}」嗎？',
  'Merge {branch} into {current}?': '要將 {branch} 合併至 {current} 嗎？',
  'Rebase Current Branch onto {branch}?': '要將目前分支 Rebase 到 {branch} 嗎？',
  'The current branch will be replayed onto "{branch}".':
      '這會將目前分支的 Commit 重放至「{branch}」。',
  'Detaches HEAD at {commit} "{subject}". Commits made in this state do not belong to a branch.':
      '會在 {commit}「{subject}」進入 detached HEAD 狀態；此後的 Commit 不屬於任何分支。',
  'Creates a new commit that reverts the changes in {commit} "{subject}".':
      '將建立新的 Commit，還原 {commit}「{subject}」造成的變更。',
  'Moves the current branch to {commit} "{subject}". Staged and working tree changes are kept.':
      '將目前分支移至 {commit}「{subject}」，保留已暫存及工作目錄變更。',
  'Moves the current branch to {commit} "{subject}". Working tree changes are kept but become unstaged.':
      '將目前分支移至 {commit}「{subject}」，工作目錄變更保留但會取消暫存。',
  'Moves the current branch to {commit} "{subject}" and discards all working tree changes. This cannot be undone.':
      '將目前分支移至 {commit}「{subject}」，並捨棄工作目錄的所有變更。此操作無法復原。',
  'Removes {commit} "{subject}" from the current branch. This cannot be undone.':
      '這會從目前分支移除 {commit}「{subject}」，且無法復原。',
  'Merge {commit} into {branch}?': '要將 {commit} 合併至 {branch} 嗎？',
  'Rebase {branch} onto {commit} "{subject}". This rewrites the current branch history.':
      '要將 {branch} Rebase 到 {commit}「{subject}」。這會重寫目前分支的歷史紀錄。',
};

const Map<String, String> _simplifiedChineseGitHistory = <String, String>{
  'Compare with Branch...': '与其他分支比较…',
  'Rename Branch': '重命名分支',
  'Delete Branch?': '要删除此分支吗？',
  'Force Delete Branch?': '要强制删除此分支吗？',
  'Delete Tag?': '要删除标签吗？',
  'Discard All Changes?': '要丢弃所有更改吗？',
  'Delete Forcefully': '强制删除',
  'Checkout Commit?': '要检出此 Commit 吗？',
  'Checkout': '检出',
  'Revert Commit?': '要还原此 Commit 吗？',
  'Revert': '还原',
  'Reset Current Branch?': '要重置当前分支吗？',
  'Commit Hash': 'Commit 哈希值',
  'Commit Subject': 'Commit 标题',
  'This deletes the remote branch and cannot be undone.': '这会删除远程分支，且无法撤销。',
  'This creates a merge or fast-forward on the current branch.':
      '这会在当前分支创建合并提交或执行快进合并。',
  'This permanently discards all staged, unstaged and untracked changes in this workspace.':
      '这会永久丢弃此工作区所有已暂存、未暂存和未跟踪的更改。',
  'Stashed changes': '已将更改存入 Stash',
  'Discarded all changes': '已丢弃所有更改',
  'Cherry pick was aborted because of conflicts.': 'Cherry-pick 因冲突而中止。',
  'Drop was aborted because of conflicts.': '移除 Commit 因冲突而中止。',
  'Merge stopped with conflicts. Resolve conflicts in Source Control.':
      '合并遇到冲突，请在源代码管理中解决。',
  'Rebase was aborted because of conflicts.': 'Rebase 因冲突而中止。',
  'Merge conflicts occurred. Resolve conflicts in Source Control.':
      '合并发生冲突，请在源代码管理中解决。',
  'Rebase conflicts were aborted. Resolve conflicts in Source Control.':
      'Rebase 因冲突而中止，请在源代码管理中检查并解决。',
  'This workspace is not a Git repository.': '此工作区不是 Git 仓库。',
  'Cannot run this action from detached HEAD.': '无法在 detached HEAD 状态下执行此操作。',
  'Remote origin was not found.': '找不到 origin 远程仓库。',
  'Nothing to commit.': '没有可提交的更改。',
  'Resolve conflicts before continuing.': '请先解决冲突再继续。',
  'Git operation failed.': 'Git 操作失败。',
  'Could not create a worktree: project not found': '无法创建 Worktree：找不到项目。',
  'Copied branch name: {branch}': '已复制分支名称：{branch}',
  'Renamed {old} to {new}': '已将 {old} 重命名为 {new}',
  'Merged {branch}': '已合并 {branch}',
  'Rebased onto {branch}': '已 Rebase 到 {branch}',
  'Created archive {path}': '已创建归档文件 {path}',
  'Pulled {branch}': '已 Pull {branch}',
  'Pushed tag {tag}': '已 Push 标签 {tag}',
  'Deleted tag {tag}': '已删除标签 {tag}',
  'Deleted branch {branch}': '已删除分支 {branch}',
  'Could not parse remote branch {branch}': '无法解析远程分支 {branch}',
  'Deleted {branch} on {remote}': '已从 {remote} 删除 {branch}',
  'Checked out {name}': '已检出 {name}',
  'Added tag {tag}': '已添加标签 {tag}',
  'Created branch {branch} at {commit}': '已在 {commit} 创建分支 {branch}',
  'Cherry-picked {commit}': '已 Cherry-pick {commit}',
  'Dropped {commit}': '已移除 Commit {commit}',
  'Current branch fast-forwarded to {commit}': '当前分支已快进到 {commit}',
  'Merge commit created from {commit}': '已从 {commit} 创建合并提交',
  'Rebased {branch} onto {commit}': '已将 {branch} Rebase 到 {commit}',
  'Created archive at {path}': '已在 {path} 创建归档文件',
  'Reverted {commit}': '已还原 {commit} 的更改',
  'Reset to {commit}': '已重置到 {commit}',
  'Created workspace {branch}': '已创建工作区 {branch}',
  'Could not switch to {branch}: project not found': '无法切换到 {branch}：找不到项目。',
  'Switched to {branch}': '已切换到 {branch}',
  '{label} copied': '已复制{label}',
  'Could not copy {label}': '无法复制{label}',
  'Delete the local branch "{branch}"?': '要删除本地分支“{branch}”吗？',
  'Branch "{branch}" was not fully merged. Force deletion removes the branch reference and cannot be undone.':
      '分支“{branch}”尚未完全合并。强制删除会移除分支引用，且无法撤销。',
  'Delete {branch} on {remote}?': '要从 {remote} 删除分支 {branch} 吗？',
  'Delete the local tag "{tag}"?': '要删除本地标签“{tag}”吗？',
  'Merge {branch} into {current}?': '要将 {branch} 合并到 {current} 吗？',
  'Rebase Current Branch onto {branch}?': '要将当前分支 Rebase 到 {branch} 吗？',
  'The current branch will be replayed onto "{branch}".':
      '这会将当前分支的 Commit 重放到“{branch}”。',
  'Detaches HEAD at {commit} "{subject}". Commits made in this state do not belong to a branch.':
      '将在 {commit}“{subject}”进入 detached HEAD 状态；之后的 Commit 不属于任何分支。',
  'Creates a new commit that reverts the changes in {commit} "{subject}".':
      '将创建新 Commit，还原 {commit}“{subject}”中的更改。',
  'Moves the current branch to {commit} "{subject}". Staged and working tree changes are kept.':
      '将当前分支移到 {commit}“{subject}”，保留已暂存及工作区更改。',
  'Moves the current branch to {commit} "{subject}". Working tree changes are kept but become unstaged.':
      '将当前分支移到 {commit}“{subject}”，保留工作区更改但会取消暂存。',
  'Moves the current branch to {commit} "{subject}" and discards all working tree changes. This cannot be undone.':
      '将当前分支移到 {commit}“{subject}”，并丢弃工作区中的全部更改。此操作无法撤销。',
  'Removes {commit} "{subject}" from the current branch. This cannot be undone.':
      '这会从当前分支移除 {commit}“{subject}”，且无法撤销。',
  'Merge {commit} into {branch}?': '要将 {commit} 合并到 {branch} 吗？',
  'Rebase {branch} onto {commit} "{subject}". This rewrites the current branch history.':
      '要将 {branch} Rebase 到 {commit}“{subject}”。这会重写当前分支的历史记录。',
};

const Map<String, String> _japaneseGitHistory = <String, String>{
  'Compare with Branch...': '別のブランチと比較…',
  'Rename Branch': 'ブランチ名を変更',
  'Delete Branch?': 'このブランチを削除しますか？',
  'Force Delete Branch?': 'このブランチを強制削除しますか？',
  'Delete Tag?': 'このタグを削除しますか？',
  'Discard All Changes?': 'すべての変更を破棄しますか？',
  'Delete Forcefully': '強制削除',
  'Checkout Commit?': 'このコミットをチェックアウトしますか？',
  'Checkout': 'チェックアウト',
  'Revert Commit?': 'このコミットを Revert しますか？',
  'Revert': 'Revert',
  'Reset Current Branch?': '現在のブランチをリセットしますか？',
  'Commit Hash': 'コミットハッシュ',
  'Commit Subject': 'コミットの件名',
  'This deletes the remote branch and cannot be undone.':
      'リモートブランチを削除します。この操作は元に戻せません。',
  'This creates a merge or fast-forward on the current branch.':
      '現在のブランチにマージコミットを作成するか、Fast-forward マージします。',
  'This permanently discards all staged, unstaged and untracked changes in this workspace.':
      'このワークスペース内のステージ済み、未ステージ、未追跡の変更をすべて完全に破棄します。',
  'Stashed changes': '変更を Stash に保存しました',
  'Discarded all changes': 'すべての変更を破棄しました',
  'Cherry pick was aborted because of conflicts.':
      '競合が発生したため Cherry-pick を中止しました。',
  'Drop was aborted because of conflicts.': '競合が発生したためコミットの削除を中止しました。',
  'Merge stopped with conflicts. Resolve conflicts in Source Control.':
      'マージで競合が発生しました。ソース管理で競合を解決してください。',
  'Rebase was aborted because of conflicts.': '競合が発生したため Rebase を中止しました。',
  'Merge conflicts occurred. Resolve conflicts in Source Control.':
      'マージの競合が発生しました。ソース管理で解決してください。',
  'Rebase conflicts were aborted. Resolve conflicts in Source Control.':
      '競合により Rebase を中止しました。ソース管理で確認し解決してください。',
  'This workspace is not a Git repository.': 'このワークスペースは Git リポジトリではありません。',
  'Cannot run this action from detached HEAD.':
      'detached HEAD の状態ではこの操作を実行できません。',
  'Remote origin was not found.': 'リモート origin が見つかりません。',
  'Nothing to commit.': 'コミットする変更はありません。',
  'Resolve conflicts before continuing.': '続行する前に競合を解決してください。',
  'Git operation failed.': 'Git の操作に失敗しました。',
  'Could not create a worktree: project not found':
      'Worktree を作成できませんでした：プロジェクトが見つかりません。',
  'Copied branch name: {branch}': 'ブランチ名をコピーしました：{branch}',
  'Renamed {old} to {new}': '{old} を {new} に名前変更しました',
  'Merged {branch}': '{branch} をマージしました',
  'Rebased onto {branch}': '{branch} に Rebase しました',
  'Created archive {path}': 'アーカイブ {path} を作成しました',
  'Pulled {branch}': '{branch} を Pull しました',
  'Pushed tag {tag}': 'タグ {tag} を Push しました',
  'Deleted tag {tag}': 'タグ {tag} を削除しました',
  'Deleted branch {branch}': 'ブランチ {branch} を削除しました',
  'Could not parse remote branch {branch}': 'リモートブランチ {branch} を解析できませんでした',
  'Deleted {branch} on {remote}': '{remote} の {branch} を削除しました',
  'Checked out {name}': '{name} をチェックアウトしました',
  'Added tag {tag}': 'タグ {tag} を追加しました',
  'Created branch {branch} at {commit}': '{commit} にブランチ {branch} を作成しました',
  'Cherry-picked {commit}': '{commit} を Cherry-pick しました',
  'Dropped {commit}': 'コミット {commit} を削除しました',
  'Current branch fast-forwarded to {commit}':
      '現在のブランチを {commit} に Fast-forward しました',
  'Merge commit created from {commit}': '{commit} からマージコミットを作成しました',
  'Rebased {branch} onto {commit}': '{branch} を {commit} に Rebase しました',
  'Created archive at {path}': '{path} にアーカイブを作成しました',
  'Reverted {commit}': '{commit} を Revert しました',
  'Reset to {commit}': '{commit} にリセットしました',
  'Created workspace {branch}': 'ワークスペース {branch} を作成しました',
  'Could not switch to {branch}: project not found':
      '{branch} に切り替えられません：プロジェクトが見つかりません。',
  'Switched to {branch}': '{branch} に切り替えました',
  '{label} copied': '{label}をコピーしました',
  'Could not copy {label}': '{label}をコピーできませんでした',
  'Delete the local branch "{branch}"?': 'ローカルブランチ「{branch}」を削除しますか？',
  'Branch "{branch}" was not fully merged. Force deletion removes the branch reference and cannot be undone.':
      'ブランチ「{branch}」は完全にマージされていません。強制削除はブランチ参照を削除し、元に戻せません。',
  'Delete {branch} on {remote}?': '{remote} のブランチ {branch} を削除しますか？',
  'Delete the local tag "{tag}"?': 'ローカルタグ「{tag}」を削除しますか？',
  'Merge {branch} into {current}?': '{branch} を {current} にマージしますか？',
  'Rebase Current Branch onto {branch}?': '現在のブランチを {branch} に Rebase しますか？',
  'The current branch will be replayed onto "{branch}".':
      '現在のブランチのコミットを「{branch}」上に再適用します。',
  'Detaches HEAD at {commit} "{subject}". Commits made in this state do not belong to a branch.':
      '{commit}「{subject}」で HEAD を切り離します。この状態で作成したコミットはブランチに属しません。',
  'Creates a new commit that reverts the changes in {commit} "{subject}".':
      '{commit}「{subject}」の変更を取り消す新しいコミットを作成します。',
  'Moves the current branch to {commit} "{subject}". Staged and working tree changes are kept.':
      '現在のブランチを {commit}「{subject}」に移動し、ステージ済みと作業ツリーの変更を保持します。',
  'Moves the current branch to {commit} "{subject}". Working tree changes are kept but become unstaged.':
      '現在のブランチを {commit}「{subject}」に移動し、作業ツリーの変更は保持しますがステージを解除します。',
  'Moves the current branch to {commit} "{subject}" and discards all working tree changes. This cannot be undone.':
      '現在のブランチを {commit}「{subject}」に移動し、作業ツリーの変更をすべて破棄します。この操作は元に戻せません。',
  'Removes {commit} "{subject}" from the current branch. This cannot be undone.':
      '現在のブランチから {commit}「{subject}」を削除します。この操作は元に戻せません。',
  'Merge {commit} into {branch}?': '{commit} を {branch} にマージしますか？',
  'Rebase {branch} onto {commit} "{subject}". This rewrites the current branch history.':
      '{branch} を {commit}「{subject}」に Rebase します。現在のブランチの履歴が書き換えられます。',
};
