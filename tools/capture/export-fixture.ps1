#Requires -Version 5.1
<#
.SYNOPSIS
    Converts a USBPcap capture into the cmh fixture text format.

.DESCRIPTION
    Emits one line per HID report of MasterHUB modules:

        <seconds> <product id> <in|out|get|set> <hex bytes, zero padding trimmed>

    in   input report (interrupt IN)
    out  output report (interrupt OUT, or SET_REPORT of type output)
    get  feature report returned by GET_REPORT
    set  feature report written with SET_REPORT

    The capture must contain the device descriptors (capture-daemon.ps1
    injects them) so that USB addresses can be mapped to product IDs.

.EXAMPLE
    tools\capture\export-fixture.ps1 captures\20261012-120610-app-kill-and-start.pcapng -Directions get,set

.EXAMPLE
    tools\capture\export-fixture.ps1 capture.pcapng -ProductId 0202 -Directions in > fixtures\inputs\knobs.txt
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [string]$Capture,

    # Restrict to one module, e.g. 0202.
    [string]$ProductId,

    # Any of in, out, get, set (comma separated).
    [string[]]$Directions = @('in', 'out', 'get', 'set'),

    # Only reports at or after / before this capture time in seconds.
    [double]$From = 0,
    [double]$To = [double]::MaxValue,

    [string]$TShark = 'C:\Program Files\Wireshark\tshark.exe'
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Capture)) { throw "Capture not found: $Capture" }
# "powershell -File" passes "get,set" as one string, so split it here.
$Directions = @($Directions | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($direction in $Directions) {
    if ('in', 'out', 'get', 'set' -notcontains $direction) { throw "Unknown direction: $direction" }
}

$fields = 'frame.time_relative', 'usb.device_address', 'usb.idVendor', 'usb.idProduct', 'usb.transfer_type',
    'usb.endpoint_address', 'usb.irp_info.direction', 'usb.bmRequestType', 'usb.setup.bRequest',
    'usb.setup.wValue', 'usb.capdata', 'usb.data_fragment', 'usb.control.Response'
# The HID dissector hides report payloads of control transfers; the plain
# USB dissector exposes them as generic data fields.
$arguments = @('-r', $Capture, '--disable-protocol', 'usbhid', '-T', 'fields', '-E', 'separator=|')
foreach ($field in $fields) { $arguments += @('-e', $field) }

function Format-Report([string]$hex) {
    $hex = $hex -replace '[:\s]', ''
    $hex = $hex -replace '(00)+$', ''
    if ($hex.Length -eq 0) { return '' }
    (($hex -split '(..)') | Where-Object { $_ }) -join ' '
}

$culture = [Globalization.CultureInfo]::InvariantCulture
$products = @{}
$pendingGet = @{}

& $TShark @arguments 2>$null | ForEach-Object {
    $v = "$_".Split('|')
    if ($v.Count -lt $fields.Count) { return }
    $time = [double]::Parse($v[0], $culture)
    $address = $v[1]
    if ($v[2] -eq '0x2516' -and $v[3]) { $products[$address] = $v[3].Substring($v[3].Length - 4) }
    if (-not $products.ContainsKey($address)) { return }
    $product = $products[$address]

    $direction = $null
    $data = $null
    if ($v[4] -eq '0x01') {
        if ($v[10]) {
            $data = $v[10]
            $direction = if ([Convert]::ToInt32($v[5], 16) -band 0x80) { 'in' } else { 'out' }
        }
    } elseif ($v[4] -eq '0x02') {
        $reportType = if ($v[9]) { ([Convert]::ToInt32($v[9], 16) -shr 8) } else { 0 }
        if ($v[6] -eq '0x00') {
            # Request stage. bRequest 1 = GET_REPORT, 9 = SET_REPORT.
            $pendingGet[$address] = ($v[7] -eq '0xa1' -and $v[8] -eq '1' -and $reportType -eq 3)
            if ($v[7] -eq '0x21' -and $v[8] -eq '9' -and $v[11]) {
                $data = $v[11]
                $direction = if ($reportType -eq 3) { 'set' } elseif ($reportType -eq 2) { 'out' } else { $null }
            }
        } elseif ($pendingGet[$address] -and $v[12]) {
            $pendingGet[$address] = $false
            $data = $v[12]
            $direction = 'get'
        }
    }

    if (-not $direction -or $Directions -notcontains $direction) { return }
    if ($ProductId -and $product -ne $ProductId.ToLowerInvariant()) { return }
    if ($time -lt $From -or $time -ge $To) { return }
    '{0,9} {1} {2} {3}' -f $time.ToString('0.000', $culture), $product, $direction, (Format-Report $data)
}
