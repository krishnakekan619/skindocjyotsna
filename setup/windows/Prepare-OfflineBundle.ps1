<#
.SYNOPSIS
    Downloads everything needed to set up a SkinDocJyotsna PC on Windows, into one folder that works offline.

.DESCRIPTION
    Run on any Windows PC that HAS internet. The result is a self-contained folder
    (USB drive or network share). On the target PC, double-click Install-DevPC.cmd,
    Install-BuildPC.cmd or Install-ClinicPC.cmd inside it. No internet needed there.

    VERSIONS ARE LOCKED. setup\windows\tools.lock records the exact version, download URL and
    SHA-256 of every installer. Normal runs download exactly those files and refuse anything
    whose checksum differs. To move to newer stable releases, run with -UpdateLock, review
    the printed changes, and keep the updated tools.lock with the code.
    The first run (no tools.lock yet) creates the lock from the newest stable releases.

    Files already present in the bundle with the right checksum are not downloaded again, so an
    interrupted run can simply be restarted.

    Bundle contents:
      installers\     Node.js MSI, Git for Windows, Rust (standalone MSI), WebView2 standalone,
                      Visual Studio Build Tools bootstrapper
      vs-buildtools\  Build Tools offline layout: ONLY the MSVC x64 compiler + Windows 11 SDK
      project-deps\   npm packages / Rust crates / Tauri tools for offline builds (if available)
      tools.lock, manifest.txt, SHA256SUMS, Install-*.cmd, Install-FromBundle.ps1, lib\

.PARAMETER OutputDir
    Where to create the bundle. Default: <repo>\dist\SkinDocJyotsna-OfflineBundle-win

.PARAMETER UpdateLock
    Pin the newest stable releases (per setup\versions.env) and rewrite tools.lock.

.PARAMETER SkipBuildTools
    Leave out the Visual Studio Build Tools layout. Such a bundle can only set up clinic PCs.

.PARAMETER ResolveOnly
    Only print what would be downloaded. Nothing large is downloaded and no lock is written.

.EXAMPLE
    .\Prepare-OfflineBundle.ps1
.EXAMPLE
    .\Prepare-OfflineBundle.ps1 -UpdateLock -OutputDir E:\SkinDocJyotsna-OfflineBundle-win
#>
[CmdletBinding()]
param(
    [string]$OutputDir = '',   # default set below: $PSScriptRoot is empty in param defaults on PowerShell 5.1
    [switch]$UpdateLock,
    [switch]$SkipBuildTools,
    [switch]$ResolveOnly
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Common.ps1')
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# ---- Resolving the newest stable releases (only used with -UpdateLock) -------------------------
function Resolve-NodeVersion([string]$Spec) {
    if ($Spec -match '^\d+\.\d+\.\d+$') { return $Spec }
    if ($Spec -notmatch '^\d+$') { throw "NODE_VERSION must be a major (e.g. 24) or an exact version (e.g. 24.11.0), got '$Spec'" }
    $index = Invoke-RestMethod -Uri 'https://nodejs.org/dist/index.json' -UseBasicParsing
    $release = $index | Where-Object { $_.version -like "v$Spec.*" -and $_.files -contains 'win-x64-msi' } | Select-Object -First 1
    if (-not $release) { throw "No Node.js $Spec.x release with a Windows x64 MSI was found" }
    return $release.version.TrimStart('v')
}

function Resolve-RustVersion([string]$Spec, [string]$MetaDir) {
    if ($Spec -match '^\d+\.\d+\.\d+$') { return $Spec }
    if ($Spec -ne 'stable') { throw "RUST_VERSION must be 'stable' or an exact version (e.g. 1.90.0), got '$Spec'" }
    $toml = Join-Path $MetaDir 'channel-rust-stable.toml'
    Invoke-Download 'https://static.rust-lang.org/dist/channel-rust-stable.toml' $toml
    $match = [regex]::Match((Get-Content -LiteralPath $toml -Raw), '(?m)^\[pkg\.rust\]\s*$[\s\S]*?^version\s*=\s*"(\d+\.\d+\.\d+)')
    if (-not $match.Success) { throw 'Could not work out the latest stable Rust version. Set RUST_VERSION to an exact version in setup\versions.env.' }
    return $match.Groups[1].Value
}

function Get-GitRelease {
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/git-for-windows/git/releases/latest' `
        -UseBasicParsing -Headers @{ 'User-Agent' = 'SkinDocJyotsna-Setup'; 'Accept' = 'application/vnd.github+json' }
    if ($release.prerelease) { throw 'The latest Git for Windows release is marked as a pre-release; refusing to pin it.' }
    $asset = $release.assets | Where-Object { $_.name -match '^Git-\d+(\.\d+)+-64-bit\.exe$' } | Select-Object -First 1
    if (-not $asset) { throw 'Could not find the 64-bit Git for Windows installer in the latest release' }
    $digest = ''
    if ($asset.PSObject.Properties['digest'] -and $asset.digest -like 'sha256:*') { $digest = $asset.digest.Substring(7) }
    return @{ Version = ([regex]::Match($asset.name, '\d+(\.\d+)+')).Value; File = $asset.name; Url = $asset.browser_download_url; Sha = $digest }
}

# ---- Download helper -----------------------------------------------------------------------------
# Downloads $Url to installers\$File unless it is already there with the expected checksum.
# $ExpectedSha: from the lock (locked mode) or from the vendor (update mode); '' = not known yet.
# Returns the file's actual SHA-256.
function Get-Installer([string]$Label, [string]$Url, [string]$File, [string]$ExpectedSha, [string]$SignerPattern, [switch]$RequireSignature) {
    Write-Step $Label
    if (-not $UpdateLock -and -not $ExpectedSha) { throw "$File has no checksum in tools.lock. Run again with -UpdateLock to pin it." }
    $path = Join-Path $script:installers $File
    if ($ExpectedSha -and (Test-Path -LiteralPath $path) -and ((Get-Sha256 $path) -eq $ExpectedSha.ToLowerInvariant())) {
        Write-Ok "Already in the bundle, checksum matches: $File"
    } else {
        Invoke-Download $Url $path
        if ($ExpectedSha) {
            try { Assert-Sha256 $path $ExpectedSha }
            catch {
                Remove-Item -LiteralPath $path -Force
                if ($UpdateLock) { throw }
                throw "$File does not match tools.lock. The vendor may have published a new build at the same address. If that is expected, run again with -UpdateLock and review the change."
            }
        }
    }
    if ($RequireSignature) { Assert-Authenticode $path $SignerPattern }
    return (Get-Sha256 $path)
}

# ---- Main ----------------------------------------------------------------------------------------
if (-not $OutputDir) { $OutputDir = Join-Path $PSScriptRoot '..\..\dist\SkinDocJyotsna-OfflineBundle-win' }
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
$policy = Read-KeyValueFile (Join-Path $PSScriptRoot '..\versions.env')
$lockPath = Join-Path $PSScriptRoot 'tools.lock'
$script:installers = Join-Path $OutputDir 'installers'
$metaDir = Join-Path $OutputDir 'meta'
if ($ResolveOnly) { $metaDir = Join-Path $env:TEMP 'skindoc-bundle-meta' }
New-Item -ItemType Directory -Path $metaDir -Force | Out-Null

$previous = @{}
if (Test-Path -LiteralPath $lockPath) { $previous = Read-KeyValueFile $lockPath }
elseif (-not $UpdateLock) {
    Write-Warn 'No tools.lock yet: pinning the newest stable releases now (same as -UpdateLock).'
    $UpdateLock = $true
}

Write-Step ($(if ($UpdateLock) { 'Resolving the newest stable releases' } else { "Using pinned versions from $lockPath" }))
$lock = [ordered]@{}
if ($UpdateLock) {
    $nodeVersion = Resolve-NodeVersion $policy['NODE_VERSION']
    $rustVersion = Resolve-RustVersion $policy['RUST_VERSION'] $metaDir
    $git = Get-GitRelease
    $nodeFile = "node-v$nodeVersion-x64.msi"
    $rustFile = "rust-$rustVersion-x86_64-pc-windows-msvc.msi"
    $lock['NODE_VERSION'] = $nodeVersion; $lock['NODE_FILE'] = $nodeFile; $lock['NODE_URL'] = "https://nodejs.org/dist/v$nodeVersion/$nodeFile"; $lock['NODE_SHA256'] = ''
    $lock['GIT_VERSION'] = $git.Version; $lock['GIT_FILE'] = $git.File; $lock['GIT_URL'] = $git.Url; $lock['GIT_SHA256'] = $git.Sha
    $lock['RUST_VERSION'] = $rustVersion; $lock['RUST_FILE'] = $rustFile; $lock['RUST_URL'] = "https://static.rust-lang.org/dist/$rustFile"; $lock['RUST_SHA256'] = ''
    $lock['WEBVIEW2_VERSION'] = ''; $lock['WEBVIEW2_FILE'] = 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'; $lock['WEBVIEW2_URL'] = $policy['WEBVIEW2_URL']; $lock['WEBVIEW2_SHA256'] = ''
    $lock['VS_BUILDTOOLS_CHANNEL'] = $policy['VS_BUILDTOOLS_CHANNEL']
    $lock['VS_BOOTSTRAPPER_FILE'] = 'vs_BuildTools.exe'
    $lock['VS_BOOTSTRAPPER_URL'] = "https://aka.ms/vs/$($policy['VS_BUILDTOOLS_CHANNEL'])/release/vs_BuildTools.exe"
    $lock['VS_BOOTSTRAPPER_SHA256'] = ''
    $lock['VS_COMPONENTS'] = $policy['VS_COMPONENTS']
    $lock['VS_PRODUCT_VERSION'] = ''
} else {
    foreach ($key in @('NODE_VERSION', 'NODE_FILE', 'NODE_URL', 'NODE_SHA256', 'GIT_VERSION', 'GIT_FILE', 'GIT_URL', 'GIT_SHA256',
            'RUST_VERSION', 'RUST_FILE', 'RUST_URL', 'RUST_SHA256', 'WEBVIEW2_VERSION', 'WEBVIEW2_FILE', 'WEBVIEW2_URL', 'WEBVIEW2_SHA256',
            'VS_BUILDTOOLS_CHANNEL', 'VS_BOOTSTRAPPER_FILE', 'VS_BOOTSTRAPPER_URL', 'VS_BOOTSTRAPPER_SHA256', 'VS_COMPONENTS', 'VS_PRODUCT_VERSION')) {
        if (-not $previous.ContainsKey($key)) { throw "tools.lock is missing '$key'. Run again with -UpdateLock to rebuild it." }
        $lock[$key] = $previous[$key]
    }
}

# Rust standalone archive (no installer, no admin rights) for PCs where msiexec is blocked.
# Added to an existing lock automatically, for the SAME locked Rust version.
$lockChanged = $false
$rustArchiveFile = "rust-$($lock['RUST_VERSION'])-x86_64-pc-windows-msvc.tar.xz"
if (-not $UpdateLock -and $previous.ContainsKey('RUST_ARCHIVE_SHA256') -and $previous['RUST_ARCHIVE_FILE'] -eq $rustArchiveFile) {
    foreach ($key in 'RUST_ARCHIVE_FILE', 'RUST_ARCHIVE_URL', 'RUST_ARCHIVE_SHA256') { $lock[$key] = $previous[$key] }
} else {
    if (-not $UpdateLock) { Write-Info "Adding the Rust $($lock['RUST_VERSION']) standalone archive to tools.lock (no-admin install option)." }
    $lock['RUST_ARCHIVE_FILE'] = $rustArchiveFile
    $lock['RUST_ARCHIVE_URL'] = "https://static.rust-lang.org/dist/$rustArchiveFile"
    $lock['RUST_ARCHIVE_SHA256'] = ''
    $lockChanged = $true
}

foreach ($item in @(
        @('Node.js', 'NODE'), @('Git for Windows', 'GIT'), @('Rust (MSVC)', 'RUST'), @('WebView2 (standalone)', 'WEBVIEW2'))) {
    Write-Info ('{0,-22} {1,-12} {2}' -f $item[0], $lock["$($item[1])_VERSION"], $lock["$($item[1])_URL"])
}
if ($SkipBuildTools) { Write-Info 'VS Build Tools         skipped (-SkipBuildTools)' }
else { Write-Info ('VS Build Tools         channel {0}: {1}' -f $lock['VS_BUILDTOOLS_CHANNEL'], $lock['VS_COMPONENTS']) }
if ($ResolveOnly) { Write-Ok 'Resolve-only mode: nothing downloaded, lock unchanged.'; exit 0 }

New-Item -ItemType Directory -Path $script:installers -Force | Out-Null
Write-Step 'Checking disk space'
Assert-FreeSpace $OutputDir $(if ($SkipBuildTools) { 2 } else { 8 })
$busy = Get-Process -Name 'vs_BuildTools', 'vs_installer', 'vs_setup_bootstrapper', 'vs_installershell' -ErrorAction SilentlyContinue
if ($busy) {
    throw ("A Visual Studio Installer process is still running ($(($busy.Name | Sort-Object -Unique) -join ', ')), probably from an earlier run. " +
        'Let it finish or end it in Task Manager (or restart the PC), then run this again.')
}

# Node.js: checksum published by nodejs.org
$expected = $lock['NODE_SHA256']
if ($UpdateLock) {
    $sums = Join-Path $metaDir 'node-SHASUMS256.txt'
    Invoke-Download "https://nodejs.org/dist/v$($lock['NODE_VERSION'])/SHASUMS256.txt" $sums
    $expected = Get-ShaFromSumsFile $sums $lock['NODE_FILE']
}
$lock['NODE_SHA256'] = Get-Installer "Node.js $($lock['NODE_VERSION'])" $lock['NODE_URL'] $lock['NODE_FILE'] $expected

# Git: GitHub asset digest (when published) + Authenticode signature
$lock['GIT_SHA256'] = Get-Installer "Git for Windows $($lock['GIT_VERSION'])" $lock['GIT_URL'] $lock['GIT_FILE'] $lock['GIT_SHA256'] $null -RequireSignature

# Rust: checksum published by rust-lang.org
$expected = $lock['RUST_SHA256']
if ($UpdateLock) {
    $shaFile = Join-Path $metaDir "$($lock['RUST_FILE']).sha256"
    Invoke-Download "$($lock['RUST_URL']).sha256" $shaFile
    $expected = Get-FirstToken $shaFile
}
$lock['RUST_SHA256'] = Get-Installer "Rust $($lock['RUST_VERSION'])" $lock['RUST_URL'] $lock['RUST_FILE'] $expected

# Rust standalone archive: checksum published by rust-lang.org when newly added
$expected = $lock['RUST_ARCHIVE_SHA256']
if (-not $expected) {
    $shaFile = Join-Path $metaDir "$($lock['RUST_ARCHIVE_FILE']).sha256"
    Invoke-Download "$($lock['RUST_ARCHIVE_URL']).sha256" $shaFile
    $expected = Get-FirstToken $shaFile
}
$lock['RUST_ARCHIVE_SHA256'] = Get-Installer "Rust $($lock['RUST_VERSION']) standalone archive (no-admin install)" `
    $lock['RUST_ARCHIVE_URL'] $lock['RUST_ARCHIVE_FILE'] $expected

# WebView2: evergreen link (content changes over time) -> Microsoft signature + locked checksum
$lock['WEBVIEW2_SHA256'] = Get-Installer 'WebView2 Runtime (Evergreen Standalone, x64)' $lock['WEBVIEW2_URL'] $lock['WEBVIEW2_FILE'] $lock['WEBVIEW2_SHA256'] '*Microsoft Corporation*' -RequireSignature
$webViewPath = Join-Path $script:installers $lock['WEBVIEW2_FILE']
if ((Get-Item -LiteralPath $webViewPath).Length -lt 50MB) {
    throw 'The WebView2 file is too small to be the standalone installer (got the online bootstrapper?). Check WEBVIEW2_URL in setup\versions.env.'
}
$lock['WEBVIEW2_VERSION'] = (Get-Item -LiteralPath $webViewPath).VersionInfo.ProductVersion

$vsLayoutRel = ''
if (-not $SkipBuildTools) {
    $lock['VS_BOOTSTRAPPER_SHA256'] = Get-Installer "Visual Studio Build Tools bootstrapper (channel $($lock['VS_BUILDTOOLS_CHANNEL']))" `
        $lock['VS_BOOTSTRAPPER_URL'] $lock['VS_BOOTSTRAPPER_FILE'] $lock['VS_BOOTSTRAPPER_SHA256'] '*Microsoft Corporation*' -RequireSignature
    Write-Step 'Visual Studio Build Tools offline layout: MSVC x64 compiler + Windows 11 SDK only'
    Write-Info 'Re-running on an existing layout only fetches what is missing.'
    $vsLayout = Join-Path $OutputDir 'vs-buildtools'
    $layoutArgs = @('--layout', $vsLayout, '--lang', 'en-US', '--wait')
    foreach ($component in ($lock['VS_COMPONENTS'] -split ',')) { $layoutArgs += @('--add', $component.Trim()) }
    [void](Invoke-Installer (Join-Path $script:installers $lock['VS_BOOTSTRAPPER_FILE']) $layoutArgs @(0))
    $catalog = Join-Path $vsLayout 'Catalog.json'
    if (Test-Path -LiteralPath $catalog) {
        $info = (Get-Content -LiteralPath $catalog -Raw | ConvertFrom-Json).info
        if ($info -and $info.PSObject.Properties['productDisplayVersion']) { $lock['VS_PRODUCT_VERSION'] = $info.productDisplayVersion }
    }
    Write-Ok "Layout ready: $vsLayout (Build Tools $($lock['VS_PRODUCT_VERSION']))"
    $vsLayoutRel = 'vs-buildtools'
}

# ---- Lock file -----------------------------------------------------------------------------------
if ($UpdateLock -or $lockChanged) {
    $header = @(
        'SkinDocJyotsna tool lock - Windows x64. Exact versions + SHA-256 of every installer in the setup bundle.',
        'Generated by Prepare-OfflineBundle.ps1 -UpdateLock. Review changes and keep this file with the code.',
        "Locked at $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'))"
    )
    Write-Step 'Updating tools.lock'
    foreach ($key in $lock.Keys) {
        $old = if ($previous.ContainsKey($key)) { $previous[$key] } else { '' }
        if ($old -ne $lock[$key] -and $key -notlike '*_URL' -and $key -notlike '*_FILE') { Write-Info ('{0,-24} {1} -> {2}' -f $key, $(if ($old) { $old } else { '(new)' }), $lock[$key]) }
    }
    Write-KeyValueFile $lockPath $header $lock
    Write-Ok "Written: $lockPath"
}

Write-Step 'Copying installer scripts and the lock into the bundle'
New-Item -ItemType Directory -Path (Join-Path $OutputDir 'lib') -Force | Out-Null
foreach ($file in @('Install-FromBundle.ps1', 'Install-DevPC.cmd', 'Install-BuildPC.cmd', 'Install-ClinicPC.cmd',
        'Install-DevPC-NoAdmin.cmd', 'Install-BuildPC-NoAdmin.cmd', 'tools.lock')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination $OutputDir -Force
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'lib\Common.ps1') -Destination (Join-Path $OutputDir 'lib') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\README.md') -Destination (Join-Path $OutputDir 'README.md') -Force
Write-Ok 'Scripts copied'

Write-Step 'Project dependencies (npm packages + Rust crates) for offline builds'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
if ((Test-Path -LiteralPath (Join-Path $repoRoot 'package-lock.json')) -and (Get-Command node -ErrorAction SilentlyContinue)) {
    & node (Join-Path $repoRoot 'scripts\offline-deps.mjs') vendor (Join-Path $OutputDir 'project-deps')
    if ($LASTEXITCODE -ne 0) { throw 'Vendoring project dependencies failed.' }
} else {
    Write-Warn 'Node.js or package-lock.json not found here: project dependencies were not added to the bundle.'
}

Write-Step 'Writing manifest.txt and SHA256SUMS'
$manifest = [ordered]@{ BUNDLE_PLATFORM = 'windows-x64'; CREATED_UTC = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ') }
foreach ($key in $lock.Keys) { $manifest[$key] = $lock[$key] }
foreach ($key in @('NODE_MIN_VERSION', 'RUST_MIN_VERSION', 'GIT_MIN_VERSION')) { $manifest[$key] = $policy[$key] }
$manifest['VS_LAYOUT_DIR'] = $vsLayoutRel
Write-KeyValueFile (Join-Path $OutputDir 'manifest.txt') @('SkinDocJyotsna offline setup bundle (Windows x64). Generated - do not edit.') $manifest

$sums = Get-ChildItem -LiteralPath $script:installers -File | Sort-Object Name | ForEach-Object { "$(Get-Sha256 $_.FullName)  installers/$($_.Name)" }
Set-Content -LiteralPath (Join-Path $OutputDir 'SHA256SUMS') -Value $sums -Encoding ASCII
Write-Ok 'Manifest and checksums written'

$sizeGb = (Get-ChildItem -LiteralPath $OutputDir -Recurse -File | Measure-Object -Property Length -Sum).Sum / 1GB
Write-Step ('Bundle ready: {0} ({1:N2} GB)' -f $OutputDir, $sizeGb)
Write-Info 'Copy this whole folder to the target PC (USB drive or network share), then double-click:'
Write-Info '  Install-DevPC.cmd     developer PC: keeps tools you already have if they are new enough'
Write-Info '  Install-BuildPC.cmd   production build PC: Node.js and Rust exactly as in tools.lock'
Write-Info '  Install-ClinicPC.cmd  clinic PC: WebView2 + the SkinDocJyotsna app'
Write-Info '  Install-DevPC-NoAdmin.cmd / Install-BuildPC-NoAdmin.cmd  no admin rights or installers blocked:'
Write-Info '                        Rust is unpacked into your user folder (other tools must already be installed)'
