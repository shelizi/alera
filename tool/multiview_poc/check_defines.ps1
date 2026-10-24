$c = [IO.File]::ReadAllText('windows/flutter/ephemeral/generated_config.cmake')
$flags = @(
  'ALERA_E2E_DESKTOP=1',
  'ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true',
  'ALERA_E2E_AUTO_SELECT_TERMINAL=true',
  'ALERA_E2E_SUPPORT_DIR=E:/Dropbox/work/alera/alera/.worktrees/exp-shared-terminal-20261023/build/windows/shared_gui_ninja/e2e_support'
)
foreach ($flag in $flags) {
  $enc = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($flag))
  if ($c.Contains($enc)) { "OK  $flag" } else { "MISS $flag" }
}
