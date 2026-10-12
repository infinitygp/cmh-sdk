#Requires -Version 5.1
<#
.SYNOPSIS
    Unprivileged client for capture-daemon.ps1.

.EXAMPLE
    tools\capture\capture.ps1 start knob1-cw
    # ... perform exactly one action on the device ...
    tools\capture\capture.ps1 stop

.EXAMPLE
    # Hot-plug experiments: follow modules that re-enumerate with new addresses.
    tools\capture\capture.ps1 start reconnect-knobs -AllDevices
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)]
    [ValidateSet('start', 'stop', 'status', 'quit')]
    [string]$Action,

    [Parameter(Position = 1)]
    [string]$Name,

    [switch]$AllDevices,

    [string]$CaptureDir,

    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
if (-not $CaptureDir) { $CaptureDir = Join-Path $PSScriptRoot '..\..\captures' }
$controlDir = Join-Path ([IO.Path]::GetFullPath($CaptureDir)) '.control'
$stateFile = Join-Path $controlDir 'state.json'

function Read-State {
    if (-not (Test-Path -LiteralPath $stateFile)) { return $null }
    try { Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json } catch { $null }
}

function Wait-State([scriptblock]$predicate) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $state = Read-State
        if ($state -and (& $predicate $state)) { return $state }
        Start-Sleep -Milliseconds 100
    }
    throw "capture daemon did not respond within $TimeoutSeconds s (is capture-daemon.ps1 running elevated?)"
}

$state = Read-State
if (-not $state -or $state.state -eq 'offline') {
    throw 'capture daemon is not running; start tools\capture\capture-daemon.ps1 from an elevated PowerShell'
}

switch ($Action) {
    'status' { $state }
    'start' {
        if ($Name -cnotmatch '^[a-z0-9][a-z0-9-]{0,62}$') {
            throw 'capture name must match ^[a-z0-9][a-z0-9-]{0,62}$'
        }
        if ($state.state -eq 'capturing') { throw "a capture is already running: $($state.name)" }
        $before = $state.updated
        @{ name = $Name; allDevices = [bool]$AllDevices } | ConvertTo-Json |
            Set-Content -LiteralPath (Join-Path $controlDir 'start.json') -Encoding UTF8
        $state = Wait-State { param($s) $s.updated -ne $before -and $s.state -in 'capturing', 'error' }
        if ($state.state -eq 'error') { throw $state.error }
        $state
    }
    'stop' {
        if ($state.state -ne 'capturing') { throw 'no capture is running' }
        Set-Content -LiteralPath (Join-Path $controlDir 'stop') -Value ''
        $state = Wait-State { param($s) $s.state -in 'idle', 'error' }
        if ($state.state -eq 'error') { throw $state.error }
        $state.last
    }
    'quit' {
        Set-Content -LiteralPath (Join-Path $controlDir 'quit') -Value ''
        Wait-State { param($s) $s.state -eq 'offline' } | Out-Null
        'capture daemon stopped'
    }
}
