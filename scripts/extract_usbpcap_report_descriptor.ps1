<#
.SYNOPSIS
Extracts a HID report descriptor from a USBPcap/Wireshark enumeration capture.

.DESCRIPTION
This helper reads an existing pcapng file through tshark and writes the HID
Report Descriptor response bytes as compact hex text suitable for:

  wheelctl moza descriptor --report-descriptor-hex-file <output>

The script is read-only. It does not capture traffic, install drivers, open HID
devices, send output reports, send feature reports, touch serial configuration,
or run firmware/DFU flows.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$InputPcapng,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Output,

    [ValidateRange(0, 255)]
    [int]$InterfaceNumber = 2,

    [ValidateRange(0, 255)]
    [int]$DescriptorIndex = 0,

    [ValidateNotNullOrEmpty()]
    [string]$TsharkPath = "C:\Program Files\Wireshark\tshark.exe"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Fail([string]$Message) {
    Write-Error $Message
    exit 1
}

function Resolve-RequiredFile([string]$Path, [string]$Description) {
    $resolved = Resolve-Path -LiteralPath $Path -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        Fail "$Description not found: $Path"
    }
    return $resolved.ProviderPath
}

function Normalize-HexBytes([string]$Value) {
    $compact = ($Value -replace "[^0-9A-Fa-f]", "").ToUpperInvariant()
    if ($compact.Length -eq 0) {
        return ""
    }
    if (($compact.Length % 2) -ne 0) {
        Fail "descriptor response has an odd number of hex digits"
    }
    return $compact
}

$pcap = Resolve-RequiredFile $InputPcapng "USBPcap/Wireshark capture"
$tshark = Resolve-RequiredFile $TsharkPath "tshark executable"

$descriptorValue = ($DescriptorIndex -bor 0x2200)
$filter = "usb.getDescriptor.Response && usb.setup.bRequest == 6 && usb.setup.wValue == 0x{0:X4} && usb.setup.wIndex == {1}" -f $descriptorValue, $InterfaceNumber

$tsharkArgs = @(
    "-r", $pcap,
    "-Y", $filter,
    "-T", "fields",
    "-e", "frame.number",
    "-e", "usb.setup.wIndex",
    "-e", "usb.getDescriptor.Response",
    "-E", "separator=|"
)

$rows = & $tshark @tsharkArgs 2>&1
if ($LASTEXITCODE -ne 0) {
    Fail "tshark failed while reading '$pcap': $($rows -join ' ')"
}

$candidates = @()
foreach ($row in $rows) {
    $line = [string]$row
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $parts = $line -split "\|", 3
    if ($parts.Count -lt 3) {
        continue
    }
    $hex = Normalize-HexBytes $parts[2]
    if ($hex.Length -eq 0) {
        continue
    }
    $candidates += [pscustomobject]@{
        Frame = $parts[0]
        Interface = $parts[1]
        Hex = $hex
        ByteLength = [int]($hex.Length / 2)
    }
}

if ($candidates.Count -eq 0) {
    Fail @"
No HID Report Descriptor response was found in '$pcap'.
Expected a USBPcap/Wireshark enumeration capture containing:
  usb.setup.bRequest == 6
  usb.setup.wValue == 0x$("{0:X4}" -f $descriptorValue)
  usb.setup.wIndex == $InterfaceNumber
  usb.getDescriptor.Response bytes

Replug the R5 while capture is running, capture the USB controller that owns the
R5, and do not import USB device/configuration/interface descriptors, Windows
HidP KDR/preparsed blobs, or wDescriptorLength summaries.
"@
}

if ($candidates.Count -gt 1) {
    $summary = ($candidates | ForEach-Object {
        "frame=$($_.Frame) interface=$($_.Interface) bytes=$($_.ByteLength)"
    }) -join "; "
    Fail "multiple HID Report Descriptor responses matched; narrow the capture or interface/index selection before importing: $summary"
}

$selected = $candidates[0]
$parent = Split-Path -Parent $Output
if (-not [string]::IsNullOrWhiteSpace($parent)) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}

Set-Content -LiteralPath $Output -Value $selected.Hex -NoNewline -Encoding ASCII

Write-Host "Extracted HID Report Descriptor from frame $($selected.Frame), interface $($selected.Interface): $($selected.ByteLength) bytes"
Write-Host "Wrote compact descriptor hex to $Output"
Write-Host "Import with: wheelctl moza descriptor --device <r5-selector> --report-descriptor-hex-file `"$Output`" --json-out <lane>\\descriptor.json --json"
