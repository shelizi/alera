param(
    [string]$OutputDirectory,
    [string]$ArchivePath,
    [switch]$SkipBuild,
    [switch]$NoArchive,
    [switch]$Force,
    [string]$FlutterPath,
    [string]$ZigPath,
    [string]$TargetPlatformToolset = 'v143'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Step([string]$Message) {
    Write-Host "`n==> $Message" -ForegroundColor Cyan
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $false)][string[]]$Arguments = @()
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
    }
}

function Resolve-BuildRepoPath {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    if ($RepoRoot -notmatch '[^\x00-\x7F]') {
        return [pscustomobject]@{
            Path = $RepoRoot
            SubstDrive = $null
        }
    }

    $parent = Split-Path -Parent $RepoRoot
    $leaf = Split-Path -Leaf $RepoRoot
    foreach ($letter in @('R', 'Q', 'P', 'O', 'N', 'M')) {
        $drive = "${letter}:"
        $existing = (& subst 2>$null | Where-Object { $_ -match "^$([regex]::Escape($drive))\\:" })
        if ($existing) {
            continue
        }

        Invoke-Checked -FilePath 'subst.exe' -Arguments @($drive, $parent)
        return [pscustomobject]@{
            Path = "$drive\$leaf"
            SubstDrive = $drive
        }
    }

    throw 'The repository path contains non-ASCII characters and no free drive letter was available for a temporary SUBST mapping.'
}

function Resolve-FlutterExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$BuildRepoRoot,
        [string]$ExplicitPath
    )

    if ($ExplicitPath) {
        $resolved = Resolve-Path -LiteralPath $ExplicitPath -ErrorAction Stop
        return $resolved.Path
    }

    $localFlutter = Join-Path $BuildRepoRoot '.tools\flutter\bin\flutter.bat'
    if (Test-Path -LiteralPath $localFlutter) {
        return $localFlutter
    }

    $flutter = Get-Command flutter -ErrorAction SilentlyContinue
    if ($null -eq $flutter) {
        throw 'Flutter was not found. Install the Alera Windows build toolchain or pass -FlutterPath.'
    }
    return $flutter.Source
}

function Copy-DirectoryContents {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Get-ChildItem -LiteralPath $Source -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $Destination -Recurse -Force
    }
}

function Convert-ToPhysicalRepoPath {
    param(
        [Parameter(Mandatory = $true)][string]$Candidate,
        [Parameter(Mandatory = $true)][string]$RepoRoot
    )

    if (Test-Path -LiteralPath $Candidate) {
        return (Resolve-Path -LiteralPath $Candidate).Path
    }

    $match = [regex]::Match($Candidate, '(?i)windows[\\/]flutter[\\/]ephemeral[\\/](.+)$')
    if ($match.Success) {
        $tail = $match.Groups[1].Value -replace '/', '\\'
        $mapped = Join-Path $RepoRoot (Join-Path 'windows\flutter\ephemeral' $tail)
        if (Test-Path -LiteralPath $mapped) {
            return (Resolve-Path -LiteralPath $mapped).Path
        }
    }

    throw "Could not map build path back to the repository: $Candidate"
}

function Get-ProjectNodeText {
    param(
        [Parameter(Mandatory = $true)][xml]$Xml,
        [Parameter(Mandatory = $true)][System.Xml.XmlNamespaceManager]$Ns,
        [Parameter(Mandatory = $true)][string]$XPath
    )

    $node = $Xml.SelectSingleNode($XPath, $Ns)
    if ($null -eq $node) {
        return $null
    }
    return $node.InnerText
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir '..\..')).Path
$defaultRoot = Join-Path $repoRoot 'build\final-link-kit'
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $defaultRoot 'Alera-Final-Link-Kit'
}
if (-not $ArchivePath) {
    $ArchivePath = Join-Path $defaultRoot 'Alera-Final-Link-Kit.zip'
}
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
$ArchivePath = [System.IO.Path]::GetFullPath($ArchivePath)

$mapping = $null
$oldFlavor = $env:ALERA_FLAVOR
$oldPath = $env:PATH
try {
    $mapping = Resolve-BuildRepoPath -RepoRoot $repoRoot
    $buildRepoRoot = $mapping.Path

    if (-not $SkipBuild) {
        Write-Step 'Building the complete Windows release on the build machine'
        $flutter = Resolve-FlutterExecutable -BuildRepoRoot $buildRepoRoot -ExplicitPath $FlutterPath

        if ($ZigPath) {
            $resolvedZig = (Resolve-Path -LiteralPath $ZigPath -ErrorAction Stop).Path
            $env:PATH = "$(Split-Path -Parent $resolvedZig);$env:PATH"
        }
        elseif ($null -eq (Get-Command zig -ErrorAction SilentlyContinue)) {
            Write-Warning 'zig is not on PATH. If Ghostty is not already cached, pass -ZigPath with Zig 0.16.0.'
        }

        $env:ALERA_FLAVOR = 'release'
        Push-Location $buildRepoRoot
        try {
            Invoke-Checked -FilePath $flutter -Arguments @(
                'build', 'windows', '--release', '--dart-define=ALERA_FLAVOR=release'
            )
        }
        finally {
            Pop-Location
        }
    }

    $releaseDir = Join-Path $buildRepoRoot 'build\windows\x64\runner\Release'
    $runnerProjectPath = Join-Path $buildRepoRoot 'build\windows\x64\runner\Alera.vcxproj'
    $wrapperProjectPath = Join-Path $buildRepoRoot 'build\windows\x64\flutter\flutter_wrapper_app.vcxproj'
    $ephemeralDir = Join-Path $buildRepoRoot 'windows\flutter\ephemeral'
    if (-not (Test-Path -LiteralPath (Join-Path $releaseDir 'Alera.exe'))) {
        throw "A complete Windows release was not found at $releaseDir. Run without -SkipBuild first."
    }
    if (-not (Test-Path -LiteralPath $runnerProjectPath)) {
        throw "Generated runner project was not found: $runnerProjectPath"
    }
    if (-not (Test-Path -LiteralPath $wrapperProjectPath)) {
        throw "Generated Flutter wrapper project was not found: $wrapperProjectPath"
    }

    Write-Step 'Reading the successful generated MSBuild project'
    [xml]$projectXml = Get-Content -LiteralPath $runnerProjectPath -Raw
    $ns = New-Object System.Xml.XmlNamespaceManager($projectXml.NameTable)
    $ns.AddNamespace('m', 'http://schemas.microsoft.com/developer/msbuild/2003')

    $releaseGroup = $projectXml.SelectSingleNode("//m:ItemDefinitionGroup[contains(@Condition, 'Release|x64')]", $ns)
    if ($null -eq $releaseGroup) {
        throw 'Could not find the Release|x64 ItemDefinitionGroup in Alera.vcxproj.'
    }

    $releaseConfig = $projectXml.SelectSingleNode("//m:PropertyGroup[@Label='Configuration' and contains(@Condition, 'Release|x64')]", $ns)
    $globals = $projectXml.SelectSingleNode("//m:PropertyGroup[@Label='Globals']", $ns)
    $platformToolset = if ($releaseConfig -and $releaseConfig.PlatformToolset) { [string]$releaseConfig.PlatformToolset } else { '' }
    $windowsSdkVersion = if ($globals -and $globals.WindowsTargetPlatformVersion) { [string]$globals.WindowsTargetPlatformVersion } else { '' }

    $definitions = @()
    foreach ($definition in ([string]$releaseGroup.ClCompile.PreprocessorDefinitions -split ';')) {
        $value = $definition.Trim()
        if (-not $value -or $value -like '%(*' -or $value -like 'CMAKE_INTDIR=*') {
            continue
        }
        $definitions += $value
    }
    $definitions = @($definitions | Select-Object -Unique)

    $runnerProjectDir = Split-Path -Parent $runnerProjectPath
    $prebuiltLibraries = @()
    $systemLibraries = @()
    $resolvedPrebuilt = @{}
    foreach ($dependency in ([string]$releaseGroup.Link.AdditionalDependencies -split ';')) {
        $dep = $dependency.Trim()
        if (-not $dep -or $dep -like '%(*') {
            continue
        }

        $looksLikePath = [System.IO.Path]::IsPathRooted($dep) -or $dep.Contains('\') -or $dep.Contains('/')
        if (-not $looksLikePath) {
            $systemLibraries += $dep
            continue
        }

        $candidate = if ([System.IO.Path]::IsPathRooted($dep)) {
            $dep
        }
        else {
            [System.IO.Path]::GetFullPath((Join-Path $runnerProjectDir $dep))
        }

        if (-not (Test-Path -LiteralPath $candidate)) {
            if ([System.IO.Path]::GetFileName($dep) -ieq 'flutter_windows.dll.lib') {
                $candidate = Join-Path $ephemeralDir 'flutter_windows.dll.lib'
            }
        }
        if (-not (Test-Path -LiteralPath $candidate)) {
            throw "Prebuilt link dependency was not found: $dep (resolved as $candidate)"
        }

        $name = [System.IO.Path]::GetFileName($candidate)
        $resolvedPrebuilt[$name] = (Resolve-Path -LiteralPath $candidate).Path
        $prebuiltLibraries += $name
    }
    $prebuiltLibraries = @($prebuiltLibraries | Select-Object -Unique)
    $systemLibraries = @($systemLibraries | Select-Object -Unique)

    # flutter_wrapper_app.lib contains compiled MSVC/STL code and must not cross
    # from a newer build-machine toolset into a VS2022/v143 final link. Rebuild
    # the small wrapper locally from source instead. Plugin .lib files are tiny
    # DLL import libraries and remain safe to carry in the kit.
    $locallyCompiledLibraries = @('flutter_wrapper_app.lib')
    foreach ($localLibrary in $locallyCompiledLibraries) {
        if ($prebuiltLibraries -notcontains $localLibrary) {
            throw "Expected local-compile dependency was not found: $localLibrary"
        }
        $prebuiltLibraries = @($prebuiltLibraries | Where-Object { $_ -ine $localLibrary })
        [void]$resolvedPrebuilt.Remove($localLibrary)
    }

    $sourceCompileNodes = $projectXml.SelectNodes('//m:ItemGroup/m:ClCompile', $ns)
    $runnerSources = @()
    foreach ($node in $sourceCompileNodes) {
        $include = [string]$node.Include
        if ($include -match '(?i)windows[\\/]runner[\\/]([^\\/]+\.cpp)$') {
            $runnerSources += $matches[1]
        }
    }
    $runnerSources = @($runnerSources | Select-Object -Unique)
    if ($runnerSources.Count -eq 0) {
        throw 'No Windows runner C++ sources were discovered from Alera.vcxproj.'
    }

    [xml]$wrapperXml = Get-Content -LiteralPath $wrapperProjectPath -Raw
    $wrapperNs = New-Object System.Xml.XmlNamespaceManager($wrapperXml.NameTable)
    $wrapperNs.AddNamespace('m', 'http://schemas.microsoft.com/developer/msbuild/2003')
    $wrapperSources = @()
    foreach ($node in $wrapperXml.SelectNodes('//m:ItemGroup/m:ClCompile', $wrapperNs)) {
        $include = [string]$node.Include
        if ($include -match '(?i)cpp_client_wrapper[\\/]([^\\/]+\.cc)$') {
            $wrapperSources += $matches[1]
        }
    }
    $wrapperSources = @($wrapperSources | Select-Object -Unique)
    if ($wrapperSources.Count -eq 0) {
        throw 'No Flutter wrapper C++ sources were discovered from flutter_wrapper_app.vcxproj.'
    }

    if (Test-Path -LiteralPath $OutputDirectory) {
        if (-not $Force) {
            throw "Output directory already exists: $OutputDirectory. Re-run with -Force to replace it."
        }
        Remove-Item -LiteralPath $OutputDirectory -Recurse -Force
    }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null

    $srcRunner = Join-Path $OutputDirectory 'src\runner'
    $srcFlutter = Join-Path $OutputDirectory 'src\flutter'
    $srcFlutterWrapper = Join-Path $OutputDirectory 'src\flutter_wrapper'
    $libDir = Join-Path $OutputDirectory 'lib'
    $payloadDir = Join-Path $OutputDirectory 'payload'
    $flutterEngineInclude = Join-Path $OutputDirectory 'include\flutter_engine'
    $flutterCppInclude = Join-Path $OutputDirectory 'include\flutter_cpp'
    New-Item -ItemType Directory -Path $srcRunner, $srcFlutter, $srcFlutterWrapper, $libDir, $payloadDir, $flutterEngineInclude, $flutterCppInclude -Force | Out-Null

    Write-Step 'Collecting runner sources, Flutter headers, plugin headers, and prebuilt link libraries'
    Copy-DirectoryContents -Source (Join-Path $buildRepoRoot 'windows\runner') -Destination $srcRunner
    $testsDir = Join-Path $srcRunner 'tests'
    if (Test-Path -LiteralPath $testsDir) { Remove-Item -LiteralPath $testsDir -Recurse -Force }
    $runnerCmake = Join-Path $srcRunner 'CMakeLists.txt'
    if (Test-Path -LiteralPath $runnerCmake) { Remove-Item -LiteralPath $runnerCmake -Force }

    Copy-Item -LiteralPath (Join-Path $buildRepoRoot 'windows\flutter\generated_plugin_registrant.cc') -Destination $srcFlutter -Force
    Copy-Item -LiteralPath (Join-Path $buildRepoRoot 'windows\flutter\generated_plugin_registrant.h') -Destination $srcFlutter -Force
    Copy-DirectoryContents -Source (Join-Path $ephemeralDir 'cpp_client_wrapper') -Destination $srcFlutterWrapper

    Get-ChildItem -LiteralPath $ephemeralDir -File -Filter '*.h' | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $flutterEngineInclude -Force
    }
    Copy-DirectoryContents -Source (Join-Path $ephemeralDir 'cpp_client_wrapper\include') -Destination $flutterCppInclude

    foreach ($name in $prebuiltLibraries) {
        Copy-Item -LiteralPath $resolvedPrebuilt[$name] -Destination (Join-Path $libDir $name) -Force
    }

    $pluginIncludeRoots = @()
    $additionalIncludeText = [string]$releaseGroup.ClCompile.AdditionalIncludeDirectories
    $pluginIndex = 0
    foreach ($rawInclude in ($additionalIncludeText -split ';')) {
        $includePath = $rawInclude.Trim()
        if (-not $includePath -or $includePath -like '%(*') { continue }
        if ($includePath -notmatch '(?i)\.plugin_symlinks[\\/]') { continue }

        $physical = Convert-ToPhysicalRepoPath -Candidate $includePath -RepoRoot $repoRoot
        $destinationName = ('plugin_{0:D2}' -f $pluginIndex)
        $destination = Join-Path $OutputDirectory (Join-Path 'include\plugin_roots' $destinationName)
        Copy-DirectoryContents -Source $physical -Destination $destination
        $pluginIncludeRoots += "include\plugin_roots\$destinationName"
        $pluginIndex++
    }
    $pluginIncludeRoots = @($pluginIncludeRoots | Select-Object -Unique)

    Write-Step 'Copying the complete prebuilt runtime payload (excluding the original Alera.exe)'
    Get-ChildItem -LiteralPath $releaseDir -Force | Where-Object { $_.Name -ne 'Alera.exe' } | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $payloadDir -Recurse -Force
    }

    $pubspec = Get-Content -LiteralPath (Join-Path $repoRoot 'pubspec.yaml')
    $versionLine = $pubspec | Where-Object { $_ -match '^version:\s*(.+)$' } | Select-Object -First 1
    $version = if ($versionLine -and $versionLine -match '^version:\s*(.+)$') { $matches[1].Trim() } else { 'unknown' }
    $sourceCommit = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the source Git commit.' }
    $sourceExeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $releaseDir 'Alera.exe')).Hash

    $includeDirectories = @('src', 'src\flutter_wrapper', 'src\flutter_wrapper\include', 'include\flutter_engine', 'include\flutter_cpp') + $pluginIncludeRoots
    $projectIncludes = ($includeDirectories | ForEach-Object { '$(ProjectDir)' + ($_ -replace '/', '\') }) -join ';'
    $projectIncludes += ';%(AdditionalIncludeDirectories)'
    $projectLibraries = ($prebuiltLibraries | ForEach-Object { '$(ProjectDir)lib\' + $_ }) + $systemLibraries
    $projectLibrariesText = ($projectLibraries -join ';') + ';%(AdditionalDependencies)'
    $projectDefinitions = ($definitions -join ';') + ';%(PreprocessorDefinitions)'

    $runnerCompileItems = ($runnerSources | ForEach-Object { '    <ClCompile Include="$(ProjectDir)src\runner\' + $_ + '" />' }) -join "`r`n"
    $wrapperCompileItems = ($wrapperSources | ForEach-Object { '    <ClCompile Include="$(ProjectDir)src\flutter_wrapper\' + $_ + '" />' }) -join "`r`n"
    $projectTemplate = @'
<?xml version="1.0" encoding="utf-8"?>
<Project DefaultTargets="Build" ToolsVersion="Current" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ItemGroup Label="ProjectConfigurations">
    <ProjectConfiguration Include="Release|x64">
      <Configuration>Release</Configuration>
      <Platform>x64</Platform>
    </ProjectConfiguration>
  </ItemGroup>
  <PropertyGroup Label="Globals">
    <ProjectGuid>{A5E6C56A-A821-4C89-A7BD-3D33E16F22D1}</ProjectGuid>
    <Keyword>Win32Proj</Keyword>
    <ProjectName>AleraFinal</ProjectName>
    <WindowsTargetPlatformVersion>@@SDK@@</WindowsTargetPlatformVersion>
  </PropertyGroup>
  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.Default.props" />
  <PropertyGroup Condition="'$(Configuration)|$(Platform)'=='Release|x64'" Label="Configuration">
    <ConfigurationType>Application</ConfigurationType>
    <CharacterSet>Unicode</CharacterSet>
    <PlatformToolset>@@TARGET_TOOLSET@@</PlatformToolset>
    <WholeProgramOptimization>true</WholeProgramOptimization>
  </PropertyGroup>
  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.props" />
  <PropertyGroup>
    <OutDir Condition="'$(OutDir)'==''">$(ProjectDir)out\Alera\</OutDir>
    <IntDir Condition="'$(IntDir)'==''">$(ProjectDir).finalizer-build\obj\</IntDir>
    <TargetName>Alera</TargetName>
    <TargetExt>.exe</TargetExt>
    <LinkIncremental>false</LinkIncremental>
    <GenerateManifest>true</GenerateManifest>
  </PropertyGroup>
  <ItemDefinitionGroup Condition="'$(Configuration)|$(Platform)'=='Release|x64'">
    <ClCompile>
      <AdditionalIncludeDirectories>@@INCLUDES@@</AdditionalIncludeDirectories>
      <PreprocessorDefinitions>@@DEFINES@@</PreprocessorDefinitions>
      <WarningLevel>Level4</WarningLevel>
      <TreatWarningAsError>true</TreatWarningAsError>
      <DisableSpecificWarnings>4100</DisableSpecificWarnings>
      <ExceptionHandling>Sync</ExceptionHandling>
      <LanguageStandard>stdcpp17</LanguageStandard>
      <Optimization>MaxSpeed</Optimization>
      <RuntimeLibrary>MultiThreadedDLL</RuntimeLibrary>
      <PrecompiledHeader>NotUsing</PrecompiledHeader>
      <AdditionalOptions>/utf-8 /FS %(AdditionalOptions)</AdditionalOptions>
    </ClCompile>
    <ResourceCompile>
      <AdditionalIncludeDirectories>$(ProjectDir)src\runner;%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>
      <PreprocessorDefinitions>@@DEFINES@@</PreprocessorDefinitions>
    </ResourceCompile>
    <Link>
      <AdditionalDependencies>@@LIBRARIES@@</AdditionalDependencies>
      <SubSystem>Windows</SubSystem>
      <GenerateDebugInformation>false</GenerateDebugInformation>
      <OptimizeReferences>true</OptimizeReferences>
      <EnableCOMDATFolding>true</EnableCOMDATFolding>
      <AdditionalOptions>/machine:x64 %(AdditionalOptions)</AdditionalOptions>
    </Link>
    <Manifest>
      <AdditionalManifestFiles>$(ProjectDir)src\runner\runner.exe.manifest</AdditionalManifestFiles>
    </Manifest>
  </ItemDefinitionGroup>
  <ItemGroup>
@@RUNNER_SOURCES@@
@@WRAPPER_SOURCES@@
    <ClCompile Include="$(ProjectDir)src\flutter\generated_plugin_registrant.cc" />
    <ResourceCompile Include="$(ProjectDir)src\runner\Runner.rc" />
  </ItemGroup>
  <Import Project="$(VCTargetsPath)\Microsoft.Cpp.targets" />
</Project>
'@

    $projectText = $projectTemplate.Replace('@@SDK@@', [System.Security.SecurityElement]::Escape($windowsSdkVersion))
    $projectText = $projectText.Replace('@@TARGET_TOOLSET@@', [System.Security.SecurityElement]::Escape($TargetPlatformToolset))
    $projectText = $projectText.Replace('@@INCLUDES@@', [System.Security.SecurityElement]::Escape($projectIncludes))
    $projectText = $projectText.Replace('@@DEFINES@@', [System.Security.SecurityElement]::Escape($projectDefinitions))
    $projectText = $projectText.Replace('@@LIBRARIES@@', [System.Security.SecurityElement]::Escape($projectLibrariesText))
    $projectText = $projectText.Replace('@@RUNNER_SOURCES@@', $runnerCompileItems)
    $projectText = $projectText.Replace('@@WRAPPER_SOURCES@@', $wrapperCompileItems)
    Set-Content -LiteralPath (Join-Path $OutputDirectory 'AleraFinal.vcxproj') -Value $projectText -Encoding UTF8

    Copy-Item -LiteralPath (Join-Path $scriptDir 'finalize_windows.ps1') -Destination (Join-Path $OutputDirectory 'finalize_windows.ps1') -Force

    $manifest = [ordered]@{
        schemaVersion = 2
        product = 'Alera'
        architecture = 'x64'
        version = $version
        sourceCommit = $sourceCommit
        sourceRunnerSha256 = $sourceExeHash
        sourcePlatformToolset = $platformToolset
        targetPlatformToolset = $TargetPlatformToolset
        sourceWindowsSdkVersion = $windowsSdkVersion
        prebuiltLibraries = $prebuiltLibraries
        locallyCompiledLibraries = $locallyCompiledLibraries
        systemLibraries = $systemLibraries
        compilerDefinitions = $definitions
        includeDirectories = $includeDirectories
        runnerSources = $runnerSources
        flutterWrapperSources = $wrapperSources
        payloadDirectory = 'payload'
        outputExecutable = 'Alera.exe'
        notes = @(
            'Flutter AOT, Rust, Ghostty, native assets, plugin DLLs, and plugin import libraries are prebuilt.',
            'The target machine recompiles the small Flutter C++ wrapper and Windows runner with the target MSVC toolset.',
            'flutter_wrapper_app.lib is intentionally excluded so no newer-toolset C++ object code crosses into a VS2022/v143 final link.'
        )
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'kit-manifest.json') -Encoding UTF8

    Write-Step 'Writing transfer-integrity hashes'
    $hashEntries = Get-ChildItem -LiteralPath $OutputDirectory -Recurse -File | Where-Object { $_.Name -ne 'sha256sums.json' } | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($OutputDirectory.Length).TrimStart('\') -replace '\\', '/'
            size = $_.Length
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash
        }
    }
    $hashEntries | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'sha256sums.json') -Encoding UTF8

    $forbiddenProjectTokens = @('flutter_assemble', 'cargo build', 'zig build', 'cmake --build')
    $standaloneProject = Get-Content -LiteralPath (Join-Path $OutputDirectory 'AleraFinal.vcxproj') -Raw
    foreach ($token in $forbiddenProjectTokens) {
        if ($standaloneProject -match [regex]::Escape($token)) {
            throw "Standalone final-link project unexpectedly references a build-stage tool: $token"
        }
    }

    if (-not $NoArchive) {
        Write-Step 'Creating the transferable Final-Link Kit archive'
        $archiveParent = Split-Path -Parent $ArchivePath
        if ($archiveParent) { New-Item -ItemType Directory -Path $archiveParent -Force | Out-Null }
        if (Test-Path -LiteralPath $ArchivePath) {
            if (-not $Force) {
                throw "Archive already exists: $ArchivePath. Re-run with -Force to replace it."
            }
            Remove-Item -LiteralPath $ArchivePath -Force
        }
        Compress-Archive -Path (Join-Path $OutputDirectory '*') -DestinationPath $ArchivePath -CompressionLevel Optimal
        $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath).Hash
        Write-Host "Archive: $ArchivePath"
        Write-Host "SHA256:  $archiveHash"
    }

    Write-Host "Final-Link Kit: $OutputDirectory" -ForegroundColor Green
    Write-Host "Target command: powershell -ExecutionPolicy Bypass -File .\finalize_windows.ps1"
}
finally {
    $env:ALERA_FLAVOR = $oldFlavor
    $env:PATH = $oldPath
    if ($mapping -and $mapping.SubstDrive) {
        & subst.exe $mapping.SubstDrive /D 2>$null | Out-Null
    }
}
