#Requires -Version 5.1
# Windows bootstrap installer; Unix/macOS use install.sh.
$ErrorActionPreference = 'Stop'

$SelfRepo = 'listepo/ketch'
$BinaryName = 'ketch.exe'
$DefaultRoot = Join-Path $env:USERPROFILE '.ketch'

$TempDir = $null

function Write-Help {
    @"
Usage: install.ps1 [OPTIONS]

Install ketch, a Rust CLI for managing GitHub-released apps.

OPTIONS:
  --version <TAG>      Install specific version (default: latest)
  --root <DIR>         Ketch store root (default: $DefaultRoot)
  --install-dir <DIR>  Bootstrap location: where ketch places a link or copy
                       on your PATH (default: <root>\bin)
  --no-modify-path     Don't put the bin dir on PATH
  --help               Show this help message
"@
}

function Remove-TempDir {
    if ($null -ne $TempDir -and (Test-Path -LiteralPath $TempDir)) {
        Remove-Item -LiteralPath $TempDir -Recurse -Force
    }
}

function Resolve-CallerPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return $Path
    }
    return (Join-Path (Get-Location).Path $Path)
}

function Get-CanonicalPath {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Normalize-PathKey {
    param([string]$Path)
    return ($Path -replace '/', '\').TrimEnd('\').ToLowerInvariant()
}

function Test-UserPathHas {
    param(
        [string]$PathString,
        [string]$Dir
    )
    $dirKey = Normalize-PathKey $Dir
    foreach ($entry in ($PathString -split ';' | Where-Object { $_ })) {
        if ((Normalize-PathKey $entry) -eq $dirKey) {
            return $true
        }
    }
    return $false
}

function Add-UserPathEntry {
    param([string]$Dir)
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $current) {
        $current = ''
    }
    if (Test-UserPathHas $current $Dir) {
        return $false
    }
    $trimmed = $Dir.TrimEnd('\')
    if ($current -eq '') {
        $next = $trimmed
    }
    else {
        $next = "$trimmed;$current"
    }
    [Environment]::SetEnvironmentVariable('Path', $next, 'User')
    return $true
}

function Write-Status {
    param([string]$Message)
    Write-Host $Message
}

function Write-ErrorStatus {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Red
}

# Parse arguments. $InstallDir stays empty unless given so we can tell an
# explicit --install-dir from the default <root>\bin after $Root is resolved.
$Version = ''
$Root = ''
$InstallDir = ''
$InstallDirExplicit = $false
$NoModifyPath = $false

$argsList = @($args)
$i = 0
while ($i -lt $argsList.Count) {
    switch ($argsList[$i]) {
        '--version' {
            $i++
            if ($i -ge $argsList.Count) {
                Write-ErrorStatus 'Error: --version requires a value.'
                exit 1
            }
            $Version = $argsList[$i]
        }
        '--root' {
            $i++
            if ($i -ge $argsList.Count) {
                Write-ErrorStatus 'Error: --root requires a value.'
                exit 1
            }
            $Root = $argsList[$i]
        }
        '--install-dir' {
            $i++
            if ($i -ge $argsList.Count) {
                Write-ErrorStatus 'Error: --install-dir requires a value.'
                exit 1
            }
            $InstallDir = $argsList[$i]
            $InstallDirExplicit = $true
        }
        '--no-modify-path' {
            $NoModifyPath = $true
        }
        { $_ -in '--help', '-h' } {
            Write-Help
            exit 0
        }
        default {
            Write-ErrorStatus "Unknown option: $($argsList[$i])"
            Write-Help
            exit 1
        }
    }
    $i++
}

if ($Root -eq '') {
    $Root = $DefaultRoot
}
if ($InstallDir -eq '') {
    $InstallDir = Join-Path $Root.TrimEnd('\') 'bin'
}

# Both may be relative, and the script cds into a temp directory below: a
# relative path would be created inside it and deleted with it on exit, leaving
# nothing installed. Resolve them against the directory the user ran this in.
$Root = Resolve-CallerPath $Root
$InstallDir = Resolve-CallerPath $InstallDir

# KETCH_ROOT is only --root (or its default), never derived from --install-dir.
$env:KETCH_ROOT = $Root

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'x86' -and $env:PROCESSOR_ARCHITEW6432) {
    $arch = $env:PROCESSOR_ARCHITEW6432
}

switch ($arch) {
    'AMD64' { $TarballArch = 'x86_64' }
    'ARM64' {
        Write-ErrorStatus 'Error: Unsupported architecture: ARM64'
        Write-Host 'ketch publishes x86_64-pc-windows-msvc releases only; aarch64-pc-windows-msvc is not shipped yet.' -ForegroundColor Red
        exit 1
    }
    default {
        Write-ErrorStatus "Error: Unsupported architecture: $arch"
        exit 1
    }
}

$TarballName = "ketch-$TarballArch-pc-windows-msvc.tar.gz"

if ($Version -eq '') {
    Write-Status 'Fetching latest release...'
    if ($env:KETCH_INSTALL_RELEASE_DIR) {
        $Version = 'v9.9.9'
    }
    else {
        try {
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$SelfRepo/releases/latest"
        }
        catch {
            Write-ErrorStatus 'Error: Failed to fetch latest release.'
            exit 1
        }
        $Version = $release.tag_name
        if ([string]::IsNullOrWhiteSpace($Version)) {
            Write-ErrorStatus 'Error: Could not determine latest version.'
            exit 1
        }
    }
}

Write-Status "Installing ketch version $Version..."

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ketch-install-" + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    Push-Location $TempDir

    $TarballPath = Join-Path $TempDir 'ketch.tar.gz'
    $ChecksumsPath = Join-Path $TempDir 'SHA256SUMS'

    if ($env:KETCH_INSTALL_RELEASE_DIR) {
        Copy-Item -LiteralPath (Join-Path $env:KETCH_INSTALL_RELEASE_DIR 'payload.tar.gz') -Destination $TarballPath
        Copy-Item -LiteralPath (Join-Path $env:KETCH_INSTALL_RELEASE_DIR 'SHA256SUMS') -Destination $ChecksumsPath
    }
    else {
        $TarballUrl = "https://github.com/$SelfRepo/releases/download/$Version/$TarballName"
        $ChecksumsUrl = "https://github.com/$SelfRepo/releases/download/$Version/SHA256SUMS"
        Write-Status 'Downloading release assets...'
        try {
            Invoke-WebRequest -Uri $TarballUrl -OutFile $TarballPath
            Invoke-WebRequest -Uri $ChecksumsUrl -OutFile $ChecksumsPath
        }
        catch {
            Write-ErrorStatus "Error: Failed to download $TarballUrl"
            exit 1
        }
    }

    Write-Status 'Verifying checksum...'
    $expectedLine = Select-String -Path $ChecksumsPath -Pattern $TarballName | Select-Object -First 1
    $expectedHash = $null
    if ($null -ne $expectedLine) {
        $expectedHash = ($expectedLine.Line -split '\s+', 2)[0]
    }
    $actualHash = (Get-FileHash -LiteralPath $TarballPath -Algorithm SHA256).Hash.ToLowerInvariant()

    if ([string]::IsNullOrWhiteSpace($expectedHash)) {
        Write-ErrorStatus "Error: SHA256SUMS does not list $TarballName."
        Write-Host 'Refusing to install an unverified binary.' -ForegroundColor Red
        exit 1
    }
    if ($expectedHash -ne $actualHash) {
        Write-ErrorStatus 'Error: Checksum verification failed!'
        Write-Host "Expected: $expectedHash"
        Write-Host "Actual:   $actualHash"
        exit 1
    }

    Write-Status 'Extracting...'
    & tar -xzf $TarballPath
    if ($LASTEXITCODE -ne 0) {
        Write-ErrorStatus 'Error: Failed to extract the release archive.'
        exit 1
    }

    $BinaryPath = $null
    $candidates = @(
        (Join-Path $TempDir $BinaryName),
        (Join-Path $TempDir 'ketch' $BinaryName)
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            $BinaryPath = $candidate
            break
        }
    }
    if ($null -eq $BinaryPath) {
        $found = Get-ChildItem -Path $TempDir -Filter $BinaryName -File -Recurse | Select-Object -First 1
        if ($null -ne $found) {
            $BinaryPath = $found.FullName
        }
    }
    if ($null -eq $BinaryPath) {
        Write-ErrorStatus "Error: Could not find $BinaryName binary in archive."
        exit 1
    }

    $RootBin = Join-Path $Root 'bin'
    New-Item -ItemType Directory -Path $RootBin -Force | Out-Null
    $InstallPath = Join-Path $RootBin $BinaryName

    if (Test-Path -LiteralPath $InstallPath) {
        Write-Status 'Upgrading ketch...'
    }
    else {
        Write-Status 'Installing ketch...'
    }

    # Let ketch install itself. The downloaded binary is only used to run
    # `self install`, which fetches this same release again through ketch's own
    # pipeline: verified against SHA256SUMS, unpacked into the store, linked
    # from the bin dir and recorded like any other package.
    $selfInstall = @('self', 'install')
    if ($InstallDirExplicit) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        # ketch records the bootstrap link on the package and removes it on uninstall.
        $selfInstall += @('--link-dir', $InstallDir)
    }
    & $BinaryPath @selfInstall
    if ($LASTEXITCODE -ne 0) {
        Write-ErrorStatus "Error: ketch could not install itself into $Root."
        exit 1
    }

    $pathSet = $false
    if (-not $NoModifyPath) {
        Write-Status 'Setting up PATH...'
        Add-UserPathEntry $RootBin | Out-Null
        $pathSet = $true
    }

    Write-Host ''
    Write-Host "✓ ketch $Version installed successfully!" -ForegroundColor Green
    Write-Host ''
    Write-Host "Installed to: $InstallPath"
    Write-Host ''

    if ($pathSet) {
        Write-Host 'PATH updated. Open a new terminal to use ketch.'
    }
    else {
        Write-Host "To use ketch, add $RootBin to your PATH:"
        Write-Host "  $InstallPath path install" -ForegroundColor Green
    }

    Write-Host ''
    Write-Host 'Getting started:'
    & $InstallPath --help
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
    Remove-TempDir
}
