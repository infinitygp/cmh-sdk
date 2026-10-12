#Requires -Version 5.1
<#
.SYNOPSIS
    Elevated helper that records short, MasterHUB-only USB captures on request.

.DESCRIPTION
    USBPcap needs administrator rights. Start this script once from an elevated
    PowerShell and leave it running; unprivileged tooling (capture.ps1) then
    drives it through small request files in <CaptureDir>\.control.

    The daemon deliberately accepts nothing but a capture name: it never runs
    commands taken from a request file.

    Privacy: other devices (keyboards, storage, ...) share the root hub with
    the MasterHUB. Captures are therefore restricted to the USB addresses of
    VID 2516 devices at the source (USBPcapCMD --devices) and filtered again
    afterwards; the unfiltered temporary file is deleted.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools\capture\capture-daemon.ps1
#>
[CmdletBinding()]
param(
    [string]$CaptureDir,
    [string]$UsbPcapCmd = 'C:\Program Files\USBPcap\USBPcapCMD.exe',
    [string]$TShark = 'C:\Program Files\Wireshark\tshark.exe',
    [string]$VendorId = '0x2516',
    [int]$MaxSeconds = 300
)

$ErrorActionPreference = 'Stop'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'capture-daemon.ps1 must be started from an elevated PowerShell (USBPcap requires administrator rights).'
}
foreach ($tool in $UsbPcapCmd, $TShark) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "Required tool not found: $tool" }
}

# $PSScriptRoot is not reliably available inside param() defaults on 5.1.
if (-not $CaptureDir) { $CaptureDir = Join-Path $PSScriptRoot '..\..\captures' }
$CaptureDir = [IO.Path]::GetFullPath($CaptureDir)
$controlDir = Join-Path $CaptureDir '.control'
New-Item -ItemType Directory -Force -Path $controlDir | Out-Null
$startFile = Join-Path $controlDir 'start.json'
$stopFile = Join-Path $controlDir 'stop'
$quitFile = Join-Path $controlDir 'quit'
$stateFile = Join-Path $controlDir 'state.json'
Remove-Item -LiteralPath $startFile, $stopFile, $quitFile -ErrorAction SilentlyContinue

function Write-State([hashtable]$state) {
    $state.updated = (Get-Date).ToString('o')
    $tmp = "$stateFile.tmp"
    $state | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $tmp -Encoding UTF8
    Move-Item -LiteralPath $tmp -Destination $stateFile -Force
}

function Log([string]$message) {
    Write-Host ("[{0}] {1}" -f (Get-Date).ToString('HH:mm:ss'), $message)
}

function Start-UsbPcap([string]$interface, [string]$output, [string[]]$extraArgs) {
    $arguments = @('-d', $interface, '-o', "`"$output`"", '--inject-descriptors') + $extraArgs
    Start-Process -FilePath $UsbPcapCmd -ArgumentList $arguments -PassThru -WindowStyle Hidden
}

function Stop-UsbPcap($process) {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        $process.WaitForExit(3000) | Out-Null
    }
}

# Returns the distinct USB addresses of vendor devices seen in a capture file.
function Get-VendorAddresses([string]$pcap) {
    $lines = & $TShark -r $pcap -Y "usb.idVendor == $VendorId" -T fields -e usb.device_address -e usb.idProduct 2>$null
    $result = @{}
    foreach ($line in $lines) {
        $parts = "$line".Split("`t")
        if ($parts.Count -ge 2 -and $parts[0] -match '^\d+$') { $result[$parts[0]] = $parts[1] }
    }
    $result
}

# Finds the USBPcap root hub that hosts the MasterHUB by recording a very
# short descriptor-only probe on each interface.
function Find-MasterHub {
    $interfaces = & $UsbPcapCmd --extcap-interfaces |
        ForEach-Object { if ("$_" -match 'value=([^}]+)\}') { $Matches[1] } }
    foreach ($interface in $interfaces) {
        $probe = Join-Path $controlDir 'probe.pcap'
        Remove-Item -LiteralPath $probe -ErrorAction SilentlyContinue
        $process = Start-UsbPcap $interface $probe @('-A')
        Start-Sleep -Milliseconds 800
        Stop-UsbPcap $process
        try {
            if (Test-Path -LiteralPath $probe) {
                $addresses = Get-VendorAddresses $probe
                if ($addresses.Count -gt 0) {
                    return @{ interface = $interface; addresses = $addresses }
                }
            }
        } finally {
            Remove-Item -LiteralPath $probe -ErrorAction SilentlyContinue
        }
    }
    $null
}

$capture = $null

function Start-Capture([string]$name, [bool]$allDevices) {
    $found = Find-MasterHub
    if (-not $found) {
        Write-State @{ state = 'error'; error = "no USB device with vendor $VendorId found on any USBPcap interface" }
        Log 'no MasterHUB device found'
        return $null
    }
    $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
    $raw = Join-Path $controlDir "raw-$stamp.pcap"
    $final = Join-Path $CaptureDir "$stamp-$name.pcapng"
    # --devices keeps unrelated traffic out of the file. Hot-plug experiments
    # need -A instead because re-enumerated modules get new addresses.
    if ($allDevices) {
        $extra = @('-A')
    } else {
        $extra = @('--devices', (($found.addresses.Keys | Sort-Object { [int]$_ }) -join ','))
    }
    $process = Start-UsbPcap $found.interface $raw $extra
    $state = @{
        name = $name; raw = $raw; file = $final; process = $process
        interface = $found.interface; addresses = $found.addresses
        allDevices = $allDevices; started = Get-Date
    }
    Write-State @{
        state = 'capturing'; name = $name; file = $final; interface = $found.interface
        addresses = $found.addresses; allDevices = $allDevices; started = $state.started.ToString('o')
    }
    Log "capturing '$name' on $($found.interface), addresses: $(($found.addresses.Keys | Sort-Object { [int]$_ }) -join ',')"
    $state
}

function Stop-Capture($state, [string]$reason) {
    Stop-UsbPcap $state.process
    $packets = 0
    try {
        $addresses = @{}
        foreach ($key in $state.addresses.Keys) { $addresses[$key] = $state.addresses[$key] }
        $seen = Get-VendorAddresses $state.raw
        foreach ($key in $seen.Keys) { $addresses[$key] = $seen[$key] }
        $set = ($addresses.Keys | Sort-Object { [int]$_ }) -join ', '
        & $TShark -r $state.raw -Y "usb.device_address in {$set}" -w $state.file 2>$null
        $packets = (& $TShark -r $state.file -T fields -e frame.number 2>$null | Measure-Object).Count
        Write-State @{
            state = 'idle'; last = @{
                name = $state.name; file = $state.file; packets = $packets; reason = $reason
                interface = $state.interface; addresses = $addresses
                started = $state.started.ToString('o'); stopped = (Get-Date).ToString('o')
            }
        }
        Log "saved $($state.file) ($packets packets, $reason)"
    } catch {
        Write-State @{ state = 'error'; error = "$_" }
        Log "post-processing failed: $_"
    } finally {
        Remove-Item -LiteralPath $state.raw -ErrorAction SilentlyContinue
    }
}

Write-State @{ state = 'idle' }
Log "capture daemon ready, control directory: $controlDir"
Log 'press Ctrl+C (or run capture.ps1 quit) to exit'

try {
    while ($true) {
        if (Test-Path -LiteralPath $quitFile) {
            Remove-Item -LiteralPath $quitFile -ErrorAction SilentlyContinue
            break
        }
        if ($capture) {
            $elapsed = ((Get-Date) - $capture.started).TotalSeconds
            if (Test-Path -LiteralPath $stopFile) {
                Remove-Item -LiteralPath $stopFile -ErrorAction SilentlyContinue
                Stop-Capture $capture 'stopped'
                $capture = $null
            } elseif ($elapsed -ge $MaxSeconds) {
                Stop-Capture $capture "time limit of $MaxSeconds s"
                $capture = $null
            }
        } elseif (Test-Path -LiteralPath $startFile) {
            try {
                $request = Get-Content -LiteralPath $startFile -Raw | ConvertFrom-Json
            } catch {
                $request = $null
            }
            Remove-Item -LiteralPath $startFile, $stopFile -ErrorAction SilentlyContinue
            $name = "$($request.name)"
            if ($name -cnotmatch '^[a-z0-9][a-z0-9-]{0,62}$') {
                Write-State @{ state = 'error'; error = 'invalid capture name (allowed: lowercase letters, digits, dashes)' }
                Log 'rejected request with an invalid name'
            } else {
                $capture = Start-Capture $name ($request.allDevices -eq $true)
            }
        }
        Start-Sleep -Milliseconds 100
    }
} finally {
    if ($capture) { Stop-Capture $capture 'daemon exit' }
    Write-State @{ state = 'offline' }
    Log 'capture daemon stopped'
}
