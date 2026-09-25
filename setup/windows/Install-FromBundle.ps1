<#
.SYNOPSIS
    Sets up a Windows PC for SkinDocJyotsna from an offline bundle. No internet needed.

.DESCRIPTION
    Safe to run again: it checks what is already installed and only installs what is needed.
    A log is written to <bundle>\logs\ (or %TEMP% if the bundle folder is read-only).

    Roles:
      Dev     Developer PC. Tools that are already installed and at least the minimum version
              (setup\versions.env) are KEPT; missing or too-old tools get the locked version.
      Build   Production build PC (makes the installers that go to clinics). Node.js and Rust must
              be EXACTLY the versions in tools.lock, so every release is reproducible and traceable.
              A different version already installed stops the setup with an explanation; it is never
              silently replaced. Git, Build Tools and WebView2 follow the Dev rule.
      Clinic  Clinic PC: WebView2, plus the SkinDocJyotsna installer if it is in the bundle.

    Steps: check the bundle's SHA-256 checksums, install what is needed silently, then verify
    every tool and print a summary (installed vs locked).

.PARAMETER BundleDir
    Folder made by Prepare-OfflineBundle.ps1. Default: the folder this script is in.

.PARAMETER Role
    Dev (default), Build or Clinic.

.PARAMETER VerifyOnly
    Don't install anything, just report. No admin rights needed.

.PARAMETER Force
    Reinstall the locked versions even when suitable versions are already present.

.EXAMPLE
    .\Install-FromBundle.ps1 -Role Build
.EXAMPLE
    .\Install-FromBundle.ps1 -VerifyOnly -Role Build
#>
[CmdletBinding()]
param(
    [string]$BundleDir = '',   # default: this script's folder (set below; $PSScriptRoot is empty here on PowerShell 5.1)
    [ValidateSet('Dev', 'Build', 'Clinic')][string]$Role = 'Dev',
    [switch]$VerifyOnly,
    [switch]$Force,
    [switch]$PauseAtEnd   # used internally when the script re-launches itself elevated
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\Common.ps1')

if (-not $BundleDir) { $BundleDir = $PSScriptRoot }
$BundleDir = [IO.Path]::GetFullPath($BundleDir)
if ($BundleDir.EndsWith('\')) { $BundleDir = $BundleDir + '.' }   # "E:\" would break argument quoting

# ---- Elevation -------------------------------------------------------------------------------
if (-not $VerifyOnly -and -not (Test-IsAdmin)) {
    Write-Host 'Administrator rights are needed to install software. Windows will ask for permission...'
    $argList = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"",
        '-BundleDir', "`"$BundleDir`"", '-Role', $Role, '-PauseAtEnd')
    if ($Force) { $argList += '-Force' }
    try {
        $proc = Start-Process -FilePath 'powershell.exe' -Verb RunAs -ArgumentList $argList -Wait -PassThru
        exit $proc.ExitCode
    } catch {
        Write-Fail 'Administrator permission was not granted. Nothing was installed.'
        exit 1
    }
}

# ---- Logging ---------------------------------------------------------------------------------
$logDir = Join-Path $BundleDir 'logs'
try { New-Item -ItemType Directory -Path $logDir -Force | Out-Null; [IO.File]::WriteAllText((Join-Path $logDir '.write-test'), '') }
catch { $logDir = Join-Path $env:TEMP 'SkinDocJyotsna-setup-logs'; New-Item -ItemType Directory -Path $logDir -Force | Out-Null }
$logFile = Join-Path $logDir ("install-{0}-{1}.log" -f $Role.ToLower(), (Get-Date -Format 'yyyyMMdd-HHmmss'))
Start-Transcript -LiteralPath $logFile | Out-Null

$manifestPath = Join-Path $BundleDir 'manifest.txt'
$manifest = @{}
if (Test-Path -LiteralPath $manifestPath) { $manifest = Read-KeyValueFile $manifestPath }

$exitCode = 0
try {
    Write-Host "SkinDocJyotsna setup - role: $Role - bundle: $BundleDir" -ForegroundColor White

    # Common decision + reporting for Git, Node and Rust. Returns $true if an install is needed.
    function Test-NeedsInstall([string]$Name, [string]$InstalledText, [string]$Locked, [string]$Minimum, [bool]$Strict) {
        if ($Force) { Write-Info "-Force: installing locked $Name $Locked"; return $true }
        switch (Get-ToolAction $InstalledText $Locked $Minimum $Role $Strict) {
            'reuse' {
                $note = if ($Role -eq 'Build' -and $Strict) { 'matches tools.lock' } else { "meets minimum $Minimum" }
                Write-Ok "Using the installed $Name ($InstalledText): $note"
                return $false
            }
            'mismatch' {
                throw ("Build role needs $Name $Locked exactly (tools.lock), but $InstalledText is installed. " +
                    "Uninstall it (Apps & features) and run again, use a clean build PC/VM, or update tools.lock deliberately.")
            }
            default {
                if ($InstalledText) { Write-Info "Installed $Name ($InstalledText) is older than the minimum $Minimum; installing $Locked" }
                return $true
            }
        }
    }

    function Install-WebView2 {
        Write-Step 'Microsoft Edge WebView2 Runtime'
        $installed = Get-WebView2Version
        if ($installed -and -not $Force) { Write-Ok "Using the installed WebView2 ($installed)"; return }
        $exe = Join-Path $BundleDir "installers\$($manifest['WEBVIEW2_FILE'])"
        Assert-Authenticode $exe '*Microsoft Corporation*'
        [void](Invoke-Installer $exe @('/silent', '/install'))
        Write-Ok "Installed ($(Get-WebView2Version))"
    }

    function Install-Git {
        Write-Step "Git (locked $($manifest['GIT_VERSION']), minimum $($manifest['GIT_MIN_VERSION']))"
        if (-not (Test-NeedsInstall 'Git' (Get-ToolVersion 'git') $manifest['GIT_VERSION'] $manifest['GIT_MIN_VERSION'] $false)) { return }
        $exe = Join-Path $BundleDir "installers\$($manifest['GIT_FILE'])"
        Assert-Authenticode $exe $null
        [void](Invoke-Installer $exe @('/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES', '/CLOSEAPPLICATIONS'))
        Update-SessionPath
        Write-Ok "Installed ($(Get-ToolVersion 'git'))"
    }

    function Install-Node {
        Write-Step "Node.js (locked v$($manifest['NODE_VERSION']), minimum v$($manifest['NODE_MIN_VERSION']))"
        if (-not (Test-NeedsInstall 'Node.js' (Get-ToolVersion 'node' @('-v')) $manifest['NODE_VERSION'] $manifest['NODE_MIN_VERSION'] $true)) { return }
        $msi = Join-Path $BundleDir "installers\$($manifest['NODE_FILE'])"
        [void](Invoke-Installer 'msiexec.exe' @('/i', $msi, '/qn', '/norestart', '/l*v', (Join-Path $logDir 'node-msi.log')))
        Update-SessionPath
        Write-Ok "Installed ($(Get-ToolVersion 'node' @('-v')))"
    }

    function Install-BuildTools {
        Write-Step 'Visual Studio Build Tools: MSVC x64 compiler + Windows 11 SDK'
        $existing = Get-VcToolsPath
        if ($existing -and (Get-WindowsSdkVersion) -and -not $Force) { Write-Ok "Using the installed Build Tools ($existing)"; return }
        if (-not $manifest['VS_LAYOUT_DIR']) { throw 'This bundle was prepared with -SkipBuildTools, so it cannot set up a developer or build PC.' }
        $layout = Join-Path $BundleDir $manifest['VS_LAYOUT_DIR']
        $boot = @('vs_BuildTools.exe', 'vs_buildtools.exe', 'vs_setup.exe') |
            ForEach-Object { Join-Path $layout $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $boot) { throw "Visual Studio bootstrapper not found in $layout" }
        Assert-Authenticode $boot '*Microsoft Corporation*'
        Write-Info 'Usually takes 5-15 minutes. A progress window will appear.'
        $vsArgs = @('--noweb', '--passive', '--wait', '--norestart')
        foreach ($component in ($manifest['VS_COMPONENTS'] -split ',')) { $vsArgs += @('--add', $component.Trim()) }
        [void](Invoke-Installer $boot $vsArgs)
        $path = Get-VcToolsPath
        if (-not $path) { throw 'Build Tools installer finished but the MSVC toolset was not found.' }
        Write-Ok "Installed ($path)"
    }

    function Install-Rust {
        Write-Step "Rust (locked $($manifest['RUST_VERSION']), minimum $($manifest['RUST_MIN_VERSION']))"
        if (-not (Test-NeedsInstall 'Rust' (Get-ToolVersion 'rustc' @('-V')) $manifest['RUST_VERSION'] $manifest['RUST_MIN_VERSION'] $true)) { return }
        $msi = Join-Path $BundleDir "installers\$($manifest['RUST_FILE'])"
        [void](Invoke-Installer 'msiexec.exe' @('/i', $msi, '/qn', '/norestart', '/l*v', (Join-Path $logDir 'rust-msi.log')))
        Update-SessionPath
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            # Fallback in case the MSI did not add itself to PATH.
            $bin = Get-ChildItem -LiteralPath $env:ProgramFiles -Directory -Filter 'Rust*' -ErrorAction SilentlyContinue |
                ForEach-Object { Join-Path $_.FullName 'bin' } | Where-Object { Test-Path (Join-Path $_ 'cargo.exe') } | Select-Object -First 1
            if (-not $bin) { throw 'Rust installed but cargo.exe was not found.' }
            Add-MachinePath $bin
        }
        Write-Ok "Installed ($(Get-ToolVersion 'rustc' @('-V')))"
    }

    function Install-ClinicApp {
        Write-Step 'SkinDocJyotsna app'
        # Newest version wins (compare as versions, so 0.10.0 > 0.9.0).
        $setup = Get-ChildItem -LiteralPath (Join-Path $BundleDir 'installers') -Filter 'SkinDocJyotsna_*_x64-setup.exe' -ErrorAction SilentlyContinue |
            Sort-Object { [version]([regex]::Match($_.Name, '_(\d+\.\d+\.\d+)_').Groups[1].Value) } -Descending | Select-Object -First 1
        if (-not $setup) {
            Write-Warn 'No SkinDocJyotsna installer in this bundle yet. Add one with: node scripts\build-release.mjs --bundle <this folder>. Skipped.'
            return
        }
        Assert-Authenticode-Optional $setup.FullName
        # /S = silent. installMode is perMachine: installs to Program Files for all Windows users.
        [void](Invoke-Installer $setup.FullName @('/S'))
        Write-Ok "Installed $($setup.Name). Start it from the Start menu or the desktop shortcut."
    }

    function Show-Summary {
        Write-Step "Verification for role '$Role'"
        $strictBuild = $Role -eq 'Build'
        $rows = @(
            @{ Tool = 'WebView2 Runtime'; Roles = 'Dev Build Clinic'; Found = (Get-WebView2Version); Locked = ''; Min = '' },
            @{ Tool = 'Git'; Roles = 'Dev Build'; Found = (Get-ToolVersion 'git'); Locked = $manifest['GIT_VERSION']; Min = $manifest['GIT_MIN_VERSION'] },
            @{ Tool = 'Node.js'; Roles = 'Dev Build'; Found = (Get-ToolVersion 'node' @('-v')); Locked = $manifest['NODE_VERSION']; Min = $manifest['NODE_MIN_VERSION']; Strict = $true },
            @{ Tool = 'npm'; Roles = 'Dev Build'; Found = (Get-ToolVersion 'npm' @('-v')); Locked = ''; Min = '' },
            @{ Tool = 'MSVC Build Tools'; Roles = 'Dev Build'; Found = (Get-VcToolsPath); Locked = ''; Min = '' },
            @{ Tool = 'Windows SDK'; Roles = 'Dev Build'; Found = (Get-WindowsSdkVersion); Locked = ''; Min = '' },
            @{ Tool = 'rustc'; Roles = 'Dev Build'; Found = (Get-ToolVersion 'rustc' @('-V')); Locked = $manifest['RUST_VERSION']; Min = $manifest['RUST_MIN_VERSION']; Strict = $true },
            @{ Tool = 'cargo'; Roles = 'Dev Build'; Found = (Get-ToolVersion 'cargo' @('-V')); Locked = ''; Min = '' }
        )
        $problems = 0
        foreach ($row in $rows) {
            $needed = ($row.Roles -split ' ') -contains $Role
            $locked = if ($row.Locked) { " (locked $($row.Locked))" } else { '' }
            if (-not $row.Found) {
                if ($needed) { Write-Fail ('{0,-18} MISSING{1}' -f $row.Tool, $locked); $problems++ }
                else { Write-Info ('[--]   {0,-18} not installed (not needed for {1})' -f $row.Tool, $Role) }
                continue
            }
            $status = 'ok'
            if ($row.Locked -or $row.Min) {
                $status = Get-ToolAction $row.Found $row.Locked $row.Min $Role ([bool]($strictBuild -and $row.ContainsKey('Strict')))
            }
            switch ($status) {
                'mismatch' { Write-Fail ('{0,-18} {1}  <- must be exactly {2} for Build' -f $row.Tool, $row.Found, $row.Locked); if ($needed) { $problems++ } }
                'install' { Write-Fail ('{0,-18} {1}  <- older than minimum {2}' -f $row.Tool, $row.Found, $row.Min); if ($needed) { $problems++ } }
                default { Write-Ok ('{0,-18} {1}{2}' -f $row.Tool, $row.Found, $locked) }
            }
        }
        return $problems
    }

    if (-not $VerifyOnly) {
        Write-Step 'Checking bundle integrity'
        if (-not $manifest.Count) { throw "manifest.txt not found in $BundleDir. Is this a bundle made by Prepare-OfflineBundle.ps1?" }
        if ($manifest['BUNDLE_PLATFORM'] -ne 'windows-x64') { throw "This bundle is for '$($manifest['BUNDLE_PLATFORM'])', not windows-x64." }
        Write-Info "Bundle created $($manifest['CREATED_UTC']): Node $($manifest['NODE_VERSION']), Rust $($manifest['RUST_VERSION']), Git $($manifest['GIT_VERSION'])"
        foreach ($line in Get-Content -LiteralPath (Join-Path $BundleDir 'SHA256SUMS')) {
            $parts = $line.Trim() -split '\s+', 2
            if ($parts.Count -ne 2) { continue }
            Assert-Sha256 (Join-Path $BundleDir ($parts[1] -replace '/', '\')) $parts[0]
        }

        Install-WebView2
        if ($Role -eq 'Clinic') {
            Install-ClinicApp
        } else {
            Install-Git
            Install-Node
            Install-BuildTools
            Install-Rust
        }
    }

    $problems = Show-Summary
    if ($global:ClinicSetupRebootRequired) { Write-Warn 'A restart is needed to finish the installation. Please restart this PC.' }
    if ($problems -gt 0) {
        Write-Fail "$problems problem(s) found. See the log: $logFile"
        $exitCode = 1
    } else {
        Write-Host "`nAll required tools for '$Role' are in place." -ForegroundColor Green
        if ($Role -ne 'Clinic' -and -not $VerifyOnly) { Write-Info 'Open a NEW terminal window so the updated PATH is picked up.' }
    }
} catch {
    Write-Fail $_.Exception.Message
    Write-Fail "Setup stopped. Fix the problem above and run the script again (it resumes safely). Log: $logFile"
    $exitCode = 1
} finally {
    Stop-Transcript | Out-Null
}

if ($PauseAtEnd) { Read-Host 'Press Enter to close' | Out-Null }
exit $exitCode
