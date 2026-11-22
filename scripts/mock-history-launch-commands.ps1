# Offline Host-to-provider command boundary test only.
# These are deliberately fake, inert CLI names in the PATH of ONE fresh
# isolated Runtime Host. They never contact Claude/Codex, read real accounts,
# create a model prompt, or modify any existing provider directory.
function New-IsolatedHistoryCommandShims {
    param([Parameter(Mandatory=$true)][string]$FixtureRoot)

    if ($FixtureRoot -match '[&|<>^%!"\r\n]') {
        throw 'Unsafe mock CLI fixture path'
    }
    $directory = Join-Path $FixtureRoot 'isolated-cli-shims'
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $codexHome = Join-Path $FixtureRoot 'codex-test-account'
    $claudeHome = Join-Path $FixtureRoot 'claude-test-account'

    # Neither command can fall through to a real executable. Only an exact
    # native UUID, exact provider account path and the prewritten JSONL marker
    # can make the shim print its success sentinel.
    $codex = @'
@echo off
setlocal
if /I not "%CODEX_HOME%"=="__ACCOUNT__" if /I not "%CODEX_HOME%"=="\\?\__ACCOUNT__" exit /b 51
if /I not "%~1"=="--no-daemon" exit /b 52
if /I not "%~2"=="resume" exit /b 53
if "%~3"=="" exit /b 54
if not "%~4"=="" exit /b 55
set "FILE=%CODEX_HOME%\sessions\2026\11\21\rollout-2026-11-21T00-00-00-%~3.jsonl"
if not exist "%FILE%" exit /b 56
findstr /L /C:"alera protected history fixture codex" "%FILE%" >nul 2>nul
if errorlevel 1 exit /b 57
echo ALERA_ISOLATED_CODEX_RESUME_COMMAND_VERIFIED
exit /b 0
'@
    $claude = @'
@echo off
setlocal
if /I not "%CLAUDE_CONFIG_DIR%"=="__ACCOUNT__" if /I not "%CLAUDE_CONFIG_DIR%"=="\\?\__ACCOUNT__" exit /b 61
if /I not "%~1"=="--resume" exit /b 62
if "%~2"=="" exit /b 63
if not "%~3"=="" exit /b 64
set "FILE=%CLAUDE_CONFIG_DIR%\projects\-isolated\%~2.jsonl"
if not exist "%FILE%" exit /b 65
findstr /L /C:"alera protected history fixture claude" "%FILE%" >nul 2>nul
if errorlevel 1 exit /b 66
echo ALERA_ISOLATED_CLAUDE_RESUME_COMMAND_VERIFIED
exit /b 0
'@
    $encoding = [Text.ASCIIEncoding]::new()
    [IO.File]::WriteAllText(
        (Join-Path $directory 'codex.cmd'), $codex.Replace('__ACCOUNT__', $codexHome), $encoding
    )
    [IO.File]::WriteAllText(
        (Join-Path $directory 'claude.cmd'), $claude.Replace('__ACCOUNT__', $claudeHome), $encoding
    )
    return $directory
}
