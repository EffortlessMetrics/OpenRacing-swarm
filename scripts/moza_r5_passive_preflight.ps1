param(
    [string]$Lane = "",
    [string]$WheelbasePid = "0x0014",
    [string]$Operator = "Steven",
    [switch]$KeepLane
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-CargoTool {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    Write-Host "==> $Name"
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

function Invoke-ExpectedVerifierFailure {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LanePath
    )

    Write-Host "==> verify passive bundle negative path"
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = & cargo run -p wheelctl -- moza verify-bundle --lane $LanePath --stage passive --json 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    $text = ($output | Out-String)

    if ($text -match "panicked|thread '.*' panicked|stack backtrace") {
        throw "verify-bundle panicked instead of reporting missing artifacts.`n$text"
    }

    $looksLikeMissingArtifactFailure = $text -match "missing|manifest|artifact|receipt|success.*false"

    if ($exitCode -eq 0 -and -not $looksLikeMissingArtifactFailure) {
        throw "verify-bundle unexpectedly succeeded without a missing-artifact report.`n$text"
    }

    if ($exitCode -ne 0 -and -not $looksLikeMissingArtifactFailure) {
        throw "verify-bundle failed, but not with the expected missing-artifact shape.`n$text"
    }

    Write-Host "Expected no-hardware verifier result observed."
}

function Invoke-HardwareRailSmoke {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LanePath,
        [Parameter(Mandatory = $true)]
        [string]$OperatorName
    )

    Invoke-CargoTool "init generic hardware lane scaffold" @(
        "run", "-p", "wheelctl", "--",
        "hardware", "lane", "init",
        "--family", "moza-r5",
        "--topology", "wheelbase-hub",
        "--lane", $LanePath,
        "--operator", $OperatorName,
        "--json"
    )

    $statusPath = Join-Path $LanePath "hardware-lane-status.json"
    Invoke-CargoTool "inventory generic hardware lane scaffold" @(
        "run", "-p", "wheelctl", "--",
        "hardware", "lane", "status",
        "--lane", $LanePath,
        "--json-out", $statusPath,
        "--json"
    )

    $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
    if ($status.no_hid_device_opened -ne $true) {
        throw "hardware lane status should not open HID devices"
    }
    if ($status.no_ffb_writes -ne $true) {
        throw "hardware lane status should not write FFB"
    }
    if ($status.no_output_reports -ne $true) {
        throw "hardware lane status should not write output reports"
    }
    if ($status.no_feature_reports -ne $true) {
        throw "hardware lane status should not write feature reports"
    }
    if ($status.no_serial_config_commands -ne $true) {
        throw "hardware lane status should not touch serial config"
    }
    if ($status.no_firmware_or_dfu_commands -ne $true) {
        throw "hardware lane status should not run firmware or DFU commands"
    }
    if ($status.ready_for_zero_torque -ne $false) {
        throw "fresh scaffold must not be ready for zero torque"
    }
    if ($status.ready_for_ffb -ne $false) {
        throw "fresh scaffold must not be ready for FFB"
    }

    $safeNext = ($status.safe_next_commands -join "`n")
    if ($safeNext -match "(?i)(zero-torque|torque-test|watchdog-proof|disconnect-proof|simulator-ffb-smoke|feature-report|output-report|serial|firmware|dfu|ffb)") {
        throw "hardware lane status suggested an output-adjacent command for a fresh scaffold.`n$safeNext"
    }

    Write-Host "Expected generic hardware rail scaffold/status result observed."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if ([string]::IsNullOrWhiteSpace($Lane)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $Lane = Join-Path ([System.IO.Path]::GetTempPath()) "openracing-moza-r5-preflight-$stamp"
}

Write-Host "OpenRacing Moza R5 passive preflight"
Write-Host "Lane: $Lane"

Invoke-CargoTool "wheelctl moza help" @("run", "-p", "wheelctl", "--", "moza", "--help")
Invoke-CargoTool "hid-capture help" @("run", "-p", "racing-wheel-hid-capture", "--bin", "hid-capture", "--", "--help")

Invoke-HardwareRailSmoke -LanePath $Lane -OperatorName $Operator

Invoke-CargoTool "init temporary lane" @(
    "run", "-p", "wheelctl", "--",
    "moza", "init-lane",
    "--lane", $Lane,
    "--wheelbase-pid", $WheelbasePid,
    "--operator", $Operator
)

Invoke-ExpectedVerifierFailure -LanePath $Lane

if (-not $KeepLane -and (Test-Path -LiteralPath $Lane)) {
    Remove-Item -LiteralPath $Lane -Recurse -Force
    Write-Host "Removed temporary lane: $Lane"
} elseif ($KeepLane) {
    Write-Host "Kept temporary lane: $Lane"
}

Write-Host "Moza R5 passive preflight complete."
