<#
.SYNOPSIS
    Batch-generate screenshots for all ZIP disc packages in tmp/hyperscan_game.

.DESCRIPTION
    Runs hyperscan-emu play in screenshot mode (-S/--screenshot) for every .zip
    file found under tmp/hyperscan_game. Output PNGs are saved to docs/images/,
    named after the game file (without extension). Firmware (SPG290 internal ROM
    and HyperScan BIOS) is never bundled; pass paths or use the local tmp defaults.

    Boot timeline (approximate, reference retail media):
      300  frames — HyperScan startup frame
      900  frames — CD identify / TOC
      1600 frames — retail loading screen
      3600 frames — opening animation
    Default capture is 1800 frames (loading-screen window). Override with -Frames.

.PARAMETER InternalRom
    Path to the 32 KiB SPG290 internal ROM. Default: tmp/hyprscan/spg290.bin

.PARAMETER Bios
    Path to the 1 MiB HyperScan BIOS. Default: tmp/hyprscan/hyperscan.bin

.PARAMETER Frames
    Frames to emulate before capturing. Default: 1800. When omitted as an
    explicit override, known slow titles use tuned defaults (IWL / Marvel Heroes
    need ~3600 for a game loading screen). Supplying -Frames applies the same
    value to every game.

.PARAMETER Filter
    Optional wildcard(s) matched against the ZIP base name (e.g. 'IWL*').
    Only matching packages are captured.

.PARAMETER Binary
    Path to the hyperscan-emu binary. Default: cargo release build output.

.PARAMETER GameDir
    Directory scanned for .zip packages. Default: tmp/hyperscan_game

.PARAMETER OutDir
    Directory for PNG output. Default: docs/images
#>

param(
    [string]$InternalRom = "",
    [string]$Bios = "",
    [int]$Frames = 1800,
    [string[]]$Filter = @(),
    [string]$Binary = "",
    [string]$GameDir = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$framesSpecified = $PSBoundParameters.ContainsKey("Frames")
$repoRoot = Split-Path -Parent $PSScriptRoot

function Get-CaptureFrames {
    param(
        [string]$BaseName,
        [int]$DefaultFrames,
        [bool]$UseOverrides
    )

    if (-not $UseOverrides) {
        return $DefaultFrames
    }

    switch -Wildcard ($BaseName) {
        # 1800 shows the HyperScan system loading frame; 3600+ is blank.
        "IWL*" { return 1800 }
        "Marvel*" { return 3600 }
        default { return $DefaultFrames }
    }
}

if (-not $GameDir) {
    $GameDir = Join-Path $repoRoot "tmp\hyperscan_game"
}
if (-not $OutDir) {
    $OutDir = Join-Path $repoRoot "docs\images"
}
if (-not $InternalRom) {
    $InternalRom = Join-Path $repoRoot "tmp\hyprscan\spg290.bin"
}
if (-not $Bios) {
    $Bios = Join-Path $repoRoot "tmp\hyprscan\hyperscan.bin"
}

if (-not (Test-Path $GameDir)) {
    Write-Error "Game directory not found: $GameDir"
    exit 1
}
if (-not (Test-Path $InternalRom)) {
    Write-Error "Internal ROM not found: $InternalRom (pass -InternalRom)"
    exit 1
}
if (-not (Test-Path $Bios)) {
    Write-Error "BIOS not found: $Bios (pass -Bios)"
    exit 1
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

if (-not $Binary) {
    Write-Host "Building the latest release binary..." -ForegroundColor Yellow
    try {
        Push-Location $repoRoot
        cargo build --release -p hyperscanemu
        if ($LASTEXITCODE -ne 0) {
            throw "Release build failed with exit code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }
    $Binary = Join-Path $repoRoot "target\release\hyperscan-emu.exe"
    if (-not (Test-Path $Binary)) {
        $Binary = Join-Path $repoRoot "target\release\hyperscan-emu"
    }
    if (-not (Test-Path $Binary)) {
        Write-Error "Build succeeded but binary was not found at $Binary."
        exit 1
    }
}

Write-Host "Using binary:  $Binary"
Write-Host "Internal ROM:  $InternalRom"
Write-Host "BIOS:          $Bios"
Write-Host "Game dir:      $GameDir"
Write-Host "Output dir:    $OutDir"
if ($framesSpecified) {
    Write-Host "Frames:        $Frames"
} else {
    Write-Host "Frames:        $Frames (with title-screen overrides)"
}
if ($Filter.Count -gt 0) {
    Write-Host "Filter:        $((@($Filter | ForEach-Object { $_ -split ',' }) | ForEach-Object { $_.Trim() }) -join ', ')"
}
Write-Host ""

$games = Get-ChildItem -Path $GameDir -Filter "*.zip" -File |
         Sort-Object Name

if ($Filter.Count -gt 0) {
    # Allow "IWL*,Marvel*" as a single argument (common with powershell -File).
    $patterns = @($Filter | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    $games = @($games | Where-Object {
        $base = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
        $hit = $false
        foreach ($pattern in $patterns) {
            if ($base -like $pattern) { $hit = $true; break }
        }
        $hit
    })
}

if (-not $games -or @($games).Count -eq 0) {
    Write-Warning "No .zip files found under $GameDir"
    exit 0
}

$games = @($games)
Write-Host "Found $($games.Count) game package(s).`n"

$success = 0
$failed  = 0
$skipped = 0

foreach ($game in $games) {
    $baseName = [System.IO.Path]::GetFileNameWithoutExtension($game.Name)
    $safeName = $baseName -replace '[^A-Za-z0-9_\-\.]', '_'
    $outPath  = Join-Path $OutDir "$safeName.png"
    $captureFrames = Get-CaptureFrames $baseName $Frames (-not $framesSpecified)

    Write-Host -NoNewline "  $baseName ($captureFrames frames) ... "

    $prevEA = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        # Cheap media check first so invalid packages fail fast.
        $inspect = & $Binary inspect-disc $game.FullName 2>&1
        $inspectCode = $LASTEXITCODE
        if ($inspectCode -ne 0) {
            Write-Host "SKIP (inspect-disc exit $inspectCode)" -ForegroundColor Yellow
            $skipped++
            continue
        }

        $output = & $Binary play $InternalRom $Bios $game.FullName `
            --screenshot $outPath --screenshot-frames $captureFrames 2>&1
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            Write-Host "FAILED (exit $exitCode)" -ForegroundColor Red
            $output | Select-Object -Last 8 | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkRed }
            $failed++
        } elseif (Test-Path $outPath) {
            $size = (Get-Item $outPath).Length
            Write-Host "OK ($([math]::Round($size/1024)) KB)" -ForegroundColor Green
            $success++
        } else {
            Write-Host "FAILED (no output)" -ForegroundColor Red
            $failed++
        }
    } catch {
        Write-Host "FAILED ($_)" -ForegroundColor Red
        $failed++
    } finally {
        $ErrorActionPreference = $prevEA
    }
}

Write-Host ""
Write-Host "Done: $success succeeded, $skipped skipped, $failed failed out of $($games.Count) total."
if ($failed -gt 0) {
    exit 1
}
