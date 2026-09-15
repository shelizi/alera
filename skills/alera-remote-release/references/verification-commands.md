# Verification command templates

## Result classification

Use the process terminal status and assertions, not stderr presence, as the primary signal.

- `exit code != 0` => fatal command failure.
- `exit code == 0` plus all required Release/package assertions pass => command succeeded, even if stderr contains diagnostics.
- Explicit `warning` lines => warning only unless a required assertion later fails.
- Error-looking stderr with `exit code == 0` => call it a **non-fatal diagnostic**, not a script error. Escalate only if a required output is missing, stale, or fails verification.
- Never infer dev flavor from the string `alera-dev.exe` alone. Verify `Alera.vcxproj`, `NDEBUG`, production app identity, `kit-manifest.json`, and the fresh `app.so`.

Do not publish until the build process has exited and all assertions below pass.

Use these as templates inside the Alera repository through `home-node`. Adjust quoting to the connector/runtime, but preserve the checks.

## Git preflight

```powershell
git branch --show-current
git rev-parse main
git status --short --branch
git rev-list --left-right --count origin/main...main
```

## Release project identity

```powershell
$p='build\\windows\\x64\\runner\\Alera.vcxproj'
Select-String -Path $p -Pattern 'TargetName.*Release|NDEBUG|ALERA_APP_NAME|ALERA_APP_ID|ImportLibrary.*Release|ProgramDataBaseFile.*Release'
```

Expected Release identity:

```text
TargetName ... Alera
NDEBUG
ALERA_APP_NAME=L"Alera"
ALERA_APP_ID=L"dev.leynier.alera"
Release/Alera.lib
Release/Alera.pdb
```

## Kit manifest

```powershell
Get-Content build\\final-link-kit\\Alera-Final-Link-Kit\\kit-manifest.json -Raw
```

Check `sourceCommit` against the intended local `main` HEAD and `outputExecutable` against `Alera.exe`.

## ZIP structure and local hash

```powershell
$f='build\\final-link-kit\\Alera-Final-Link-Kit.zip'
$i=Get-Item $f
$h=Get-FileHash $f -Algorithm SHA256
Write-Output ('SIZE=' + $i.Length)
Write-Output ('SHA256=' + $h.Hash)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$z=[IO.Compression.ZipFile]::OpenRead((Resolve-Path $f))
try {
  $finalExe=$z.Entries | Where-Object { $_.FullName -match '(^|[\\/])Alera\\.exe$' }
  $app=$z.Entries | Where-Object { $_.FullName -match '^payload[\\/]data[\\/]app\\.so$' }
  $side=$z.Entries | Where-Object { $_.FullName -match '^payload[\\/]resources[\\/]alera[\\/]alera\\.exe$' }
  Write-Output ('FINAL_EXE_COUNT=' + @($finalExe).Count)
  Write-Output ('APP_SO=' + @($app).Count)
  Write-Output ('SIDECAR=' + @($side).Count)
} finally {
  $z.Dispose()
}
```

Expected:

```text
FINAL_EXE_COUNT=0
APP_SO=1
SIDECAR=1
```

## Atomic remote publish

Upload staging file:

```text
scp build/final-link-kit/Alera-Final-Link-Kit.zip neo-ai:/opt/fileServer-direct/uploads/alera/.Alera-Final-Link-Kit.uploading.zip
```

Verify the staged file with a single direct remote command, without shell variables:

```text
ssh neo-ai "sha256sum /opt/fileServer-direct/uploads/alera/.Alera-Final-Link-Kit.uploading.zip && unzip -t /opt/fileServer-direct/uploads/alera/.Alera-Final-Link-Kit.uploading.zip >/dev/null && echo ZIP_OK"
```

Only after the staged hash equals the local hash and ZIP validation succeeds:

```text
ssh neo-ai "mv -f /opt/fileServer-direct/uploads/alera/.Alera-Final-Link-Kit.uploading.zip /opt/fileServer-direct/uploads/alera/Alera-Final-Link-Kit.zip"
```

Final verification:

```text
ssh neo-ai "sha256sum /opt/fileServer-direct/uploads/alera/Alera-Final-Link-Kit.zip && unzip -t /opt/fileServer-direct/uploads/alera/Alera-Final-Link-Kit.zip >/dev/null && echo ZIP_OK"
```
