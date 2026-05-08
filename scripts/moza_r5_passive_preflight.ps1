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
    $output = & cargo run -p wheelctl -- moza verify-bundle --lane $LanePath --stage passive --json 2>&1
    $exitCode = $LASTEXITCODE
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
