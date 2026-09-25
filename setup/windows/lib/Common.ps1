# Shared helpers for the SkinDocJyotsna Windows setup scripts.
# Compatible with Windows PowerShell 5.1 (no PowerShell 7-only syntax).

function Write-Step([string]$Message) { Write-Host "`n==> $Message" -ForegroundColor Cyan }
function Write-Ok([string]$Message)   { Write-Host "    [OK]   $Message" -ForegroundColor Green }
function Write-Info([string]$Message) { Write-Host "    $Message" }
function Write-Warn([string]$Message) { Write-Host "    [WARN] $Message" -ForegroundColor Yellow }
function Write-Fail([string]$Message) { Write-Host "    [FAIL] $Message" -ForegroundColor Red }

function Test-IsAdmin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Reads a KEY=VALUE file; '#' starts a comment. Returns a hashtable.
function Read-KeyValueFile([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { throw "File not found: $Path" }
    $result = @{}
    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmed = ($line -replace '#.*$', '').Trim()
        if ($trimmed -eq '') { continue }
        $idx = $trimmed.IndexOf('=')
        if ($idx -lt 1) { throw "Invalid line in ${Path}: $line" }
        $result[$trimmed.Substring(0, $idx).Trim()] = $trimmed.Substring($idx + 1).Trim()
    }
    return $result
}

# Writes an ordered KEY=VALUE file (the lock format) with header comment lines.
function Write-KeyValueFile([string]$Path, [string[]]$Header, [System.Collections.Specialized.OrderedDictionary]$Values) {
    $lines = @($Header | ForEach-Object { "# $_" })
    foreach ($key in $Values.Keys) { $lines += "$key=$($Values[$key])" }
    Set-Content -LiteralPath $Path -Value $lines -Encoding ASCII
}

# Extracts a comparable version from tool output: "git version 2.55.0.windows.5" -> 2.55.0.5,
# "v24.21.0" -> 24.21.0, "rustc 1.98.1 (abc 2026-09-01)" -> 1.98.1. Returns $null if none.
function ConvertTo-ToolVersion([string]$Text) {
    if (-not $Text) { return $null }
    $m = [regex]::Match($Text, '(\d+)\.(\d+)\.(\d+)(?:\.windows)?(?:\.(\d+))?')
    if (-not $m.Success) { return $null }
    $parts = @($m.Groups[1].Value, $m.Groups[2].Value, $m.Groups[3].Value)
    if ($m.Groups[4].Success) { $parts += $m.Groups[4].Value }
    return [version]($parts -join '.')
}

# Decides what to do with a tool, given what is installed and the role's rules.
#   Dev   : reuse anything >= Minimum; otherwise install the locked version.
#   Build : Strict tools must equal Locked exactly; others follow the Dev rule.
# Returns 'reuse', 'install' or 'mismatch'.
function Get-ToolAction([string]$InstalledText, [string]$Locked, [string]$Minimum, [string]$Role, [bool]$Strict) {
    $installed = ConvertTo-ToolVersion $InstalledText
    if (-not $installed) { return 'install' }
    if ($Role -eq 'Build' -and $Strict) {
        if ($installed -eq (ConvertTo-ToolVersion $Locked)) { return 'reuse' }
        return 'mismatch'
    }
    if ($Minimum -and $installed -lt (ConvertTo-ToolVersion $Minimum)) { return 'install' }
    return 'reuse'
}

# Downloads to "<OutFile>.download" first and renames it into place only when complete, so an
# interrupted download never leaves a half-written file, and a file that is in use is reported clearly.
function Invoke-Download([string]$Url, [string]$OutFile) {
    $dir = Split-Path -Parent $OutFile
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $partial = "$OutFile.download"
    Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
    Write-Info "Downloading $Url"
    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($curl) {
        & $curl.Source --fail --location --retry 3 --retry-delay 5 --silent --show-error --output $partial $Url
        $code = $LASTEXITCODE
        if ($code -ne 0) {
            Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
            $hint = switch ($code) {
                23 { "Could not write to $dir. Check the drive is not full and that antivirus is not blocking the folder." }
                { $_ -in 5, 6, 7, 28, 35, 56 } { 'Network problem (no internet, proxy or firewall). Check the connection and run again.' }
                22 { 'The server refused the request (HTTP error). The URL may have changed.' }
                default { 'See the curl error above.' }
            }
            throw "Download failed (curl exit $code): $Url`n    $hint"
        }
    } else {
        $previous = $ProgressPreference
        $ProgressPreference = 'SilentlyContinue'   # progress bar makes Invoke-WebRequest very slow on 5.1
        try { Invoke-WebRequest -Uri $Url -OutFile $partial -UseBasicParsing }
        catch { Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue; throw }
        finally { $ProgressPreference = $previous }
    }
    try { Move-Item -LiteralPath $partial -Destination $OutFile -Force -ErrorAction Stop }
    catch {
        Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
        throw ("Downloaded, but could not replace $OutFile because it is in use. If a Visual Studio Installer " +
            "window or process (vs_BuildTools, vs_installer, setup.exe) is still running from an earlier run, close it " +
            "or restart the PC, then run again.")
    }
}

# Stops early with a clear message instead of failing halfway through a multi-GB download.
function Assert-FreeSpace([string]$Path, [double]$RequiredGB) {
    $root = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Path))
    $drive = New-Object IO.DriveInfo($root)
    $freeGB = [math]::Round($drive.AvailableFreeSpace / 1GB, 1)
    if ($freeGB -lt $RequiredGB) { throw "Only $freeGB GB free on $root; at least $RequiredGB GB is needed here. Free some space or use -OutputDir on another drive." }
    Write-Ok "Free space on ${root}: $freeGB GB"
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-Sha256([string]$Path, [string]$Expected) {
    $actual = Get-Sha256 $Path
    if ($actual -ne $Expected.Trim().ToLowerInvariant()) {
        throw "Checksum mismatch for $Path`n    expected: $Expected`n    actual:   $actual"
    }
    Write-Ok "Checksum verified: $(Split-Path -Leaf $Path)"
}

# Finds the hash for a file name in a "hash  filename" checksum list (e.g. Node's SHASUMS256.txt).
function Get-ShaFromSumsFile([string]$SumsFile, [string]$FileName) {
    foreach ($line in Get-Content -LiteralPath $SumsFile) {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -eq 2 -and (Split-Path -Leaf $parts[1].TrimStart('*')) -eq $FileName) { return $parts[0] }
    }
    throw "No checksum for $FileName in $SumsFile"
}

# Returns the first whitespace-separated token of a file (format of Rust's *.sha256 files).
function Get-FirstToken([string]$Path) {
    return ((Get-Content -LiteralPath $Path -Raw).Trim() -split '\s+')[0]
}

function Assert-Authenticode([string]$Path, [string]$SubjectPattern) {
    $sig = Get-AuthenticodeSignature -LiteralPath $Path
    if ($sig.Status -ne 'Valid') { throw "Invalid digital signature on $Path (status: $($sig.Status))" }
    $subject = $sig.SignerCertificate.Subject
    if ($SubjectPattern -and ($subject -notlike $SubjectPattern)) { throw "Unexpected signer on ${Path}: $subject" }
    Write-Ok "Signature valid: $(Split-Path -Leaf $Path) [$($subject -replace ',.*$', '')]"
}

# For our own installer: until a code-signing certificate is bought (DESIGN Q15) it is
# unsigned, so only warn. The SHA256SUMS check has already confirmed the file is intact.
function Assert-Authenticode-Optional([string]$Path) {
    $sig = Get-AuthenticodeSignature -LiteralPath $Path
    if ($sig.Status -eq 'Valid') { Write-Ok "Signature valid: $(Split-Path -Leaf $Path)" }
    else { Write-Warn "$(Split-Path -Leaf $Path) is not code-signed yet (checksum verified)." }
}

# Reloads PATH from the registry so tools installed by this script are usable immediately.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = (@($machine, $user) | Where-Object { $_ }) -join ';'
}

function Add-MachinePath([string]$Directory) {
    $current = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $entries = @($current -split ';' | Where-Object { $_ })
    if ($entries -notcontains $Directory) {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $Directory) -join ';'), 'Machine')
        Write-Ok "Added to system PATH: $Directory"
    }
    Update-SessionPath
}

$global:ClinicSetupRebootRequired = $false

# Runs an installer and waits. Arguments containing spaces are quoted automatically.
function Invoke-Installer([string]$FilePath, [string[]]$Arguments, [int[]]$SuccessCodes = @(0, 3010, 1641)) {
    $quoted = @($Arguments | ForEach-Object { if ($_ -match '\s' -and $_ -notmatch '^".*"$') { '"' + $_ + '"' } else { $_ } })
    Write-Info "Running: $(Split-Path -Leaf $FilePath) $($quoted -join ' ')"
    try {
        $proc = Start-Process -FilePath $FilePath -ArgumentList $quoted -Wait -PassThru -ErrorAction Stop
    } catch {
        $package = @($Arguments | Where-Object { $_ -match '\.(msi|exe)$' }) | Select-Object -First 1
        $manual = if ($package) { "Install it manually by double-clicking:`n      $package`n    then run this script again (it will detect it and continue)." } else { 'Run the installer manually, then run this script again.' }
        throw ("Windows blocked starting $(Split-Path -Leaf $FilePath): $($_.Exception.Message)`n" +
            "    On company-managed PCs, security software (Defender attack-surface rules, EDR/AppLocker) often stops scripts from launching installers.`n" +
            "    Check Windows Security > Protection history for a blocked item, or ask IT to allow it.`n    $manual")
    }
    if ($SuccessCodes -notcontains $proc.ExitCode) {
        throw "$(Split-Path -Leaf $FilePath) failed with exit code $($proc.ExitCode)"
    }
    if ($proc.ExitCode -eq 3010 -or $proc.ExitCode -eq 1641) { $global:ClinicSetupRebootRequired = $true }
    return $proc.ExitCode
}

# Returns the first output line of "<command> <args>", or $null if the command is missing or fails.
function Get-ToolVersion([string]$Command, [string[]]$Arguments = @('--version')) {
    # -CommandType Application prefers npm.cmd over npm.ps1 (which execution policy may block).
    $cmd = Get-Command $Command -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $cmd) { return $null }
    $ErrorActionPreference = 'Continue'
    try {
        $out = & $cmd.Source @Arguments 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        return [string]($out | Select-Object -First 1)
    } catch { return $null }
}

function Get-WebView2Version {
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($key in $keys) {
        $item = Get-ItemProperty -Path $key -ErrorAction SilentlyContinue
        if ($item -and $item.PSObject.Properties['pv'] -and $item.pv -and $item.pv -ne '0.0.0.0') { return [string]$item.pv }
    }
    return $null
}

# Returns the Visual Studio / Build Tools install path that has the MSVC x64 toolset, or $null.
function Get-VcToolsPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
    $path = & $vswhere -products * -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $path) { return $null }
    return [string]($path | Select-Object -First 1)
}

function Get-WindowsSdkVersion {
    $item = Get-ItemProperty -Path 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Microsoft SDKs\Windows\v10.0' -ErrorAction SilentlyContinue
    if ($item -and $item.PSObject.Properties['ProductVersion']) { return [string]$item.ProductVersion }
    return $null
}
