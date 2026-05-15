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

function HexToBytes([string]$Hex) {
    $compact = Normalize-HexBytes $Hex
    $bytes = New-Object byte[] ($compact.Length / 2)
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        $bytes[$i] = [Convert]::ToByte($compact.Substring($i * 2, 2), 16)
    }
    return $bytes
}

function BytesToHex([byte[]]$Bytes) {
    return (($Bytes | ForEach-Object { "{0:X2}" -f $_ }) -join "")
}

function Extract-UsbPcapPayloadHex([string]$FrameHex) {
    $bytes = HexToBytes $FrameHex
    if ($bytes.Length -lt 28) {
        Fail "USBPcap frame is too short to contain a pseudoheader"
    }

    $headerLength = [int]$bytes[0] -bor ([int]$bytes[1] -shl 8)
    if ($headerLength -le 0 -or $headerLength -ge $bytes.Length) {
        Fail "USBPcap frame has invalid pseudoheader length: $headerLength"
    }

    $payload = New-Object byte[] ($bytes.Length - $headerLength)
    [Array]::Copy($bytes, $headerLength, $payload, 0, $payload.Length)
    return BytesToHex $payload
}

function Read-FrameHex([string]$FrameNumber) {
    $hexDump = & $tshark "-r" $pcap "-Y" "frame.number == $FrameNumber" "-x" 2>&1
    if ($LASTEXITCODE -ne 0) {
        Fail "tshark failed while dumping frame $FrameNumber from '$pcap': $($hexDump -join ' ')"
    }

    $hex = ""
    foreach ($line in $hexDump) {
        $text = [string]$line
        if ($text -match "^\s*[0-9A-Fa-f]{4}\s+((?:[0-9A-Fa-f]{2}\s+){1,16})") {
            $hex += Normalize-HexBytes $Matches[1]
        }
    }
    if ($hex.Length -eq 0) {
        Fail "no packet bytes were found in tshark hex dump for frame $FrameNumber"
    }
    return $hex
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
    $fallbackRows = & $tshark @(
        "-r", $pcap,
        "-T", "fields",
        "-e", "frame.number",
        "-e", "_ws.col.Info",
        "-e", "usb.data_len",
        "-E", "separator=|"
    ) 2>&1
    if ($LASTEXITCODE -ne 0) {
        Fail "tshark failed while scanning '$pcap' for HID Report Descriptor response frames: $($fallbackRows -join ' ')"
    }

    foreach ($row in $fallbackRows) {
        $line = [string]$row
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $parts = $line -split "\|", 3
        if ($parts.Count -lt 2 -or $parts[1] -ne "GET DESCRIPTOR Response HID Report") {
            continue
        }

        $hex = Extract-UsbPcapPayloadHex (Read-FrameHex $parts[0])
        if ($hex.Length -eq 0) {
            continue
        }
        $candidates += [pscustomobject]@{
            Frame = $parts[0]
            Interface = $InterfaceNumber
            Hex = $hex
            ByteLength = [int]($hex.Length / 2)
        }
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
