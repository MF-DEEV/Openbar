# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Install an already-built app for this user without replacing their settings.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [switch]$EnableAutostart
)

$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $ExecutablePath).ProviderPath
$installDir = Join-Path $env:LOCALAPPDATA 'Vorssaint'
$target = Join-Path $installDir 'Vorssaint.exe'
if ([string]::Equals($binary, $target, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Build the replacement separately from the installed app.'
}

# A hidden launch does not fix a console-subsystem binary: Explorer and the Run
# key would still create a terminal, and closing it would terminate the island.
$bytes = [IO.File]::ReadAllBytes($binary)
if ($bytes.Length -lt 64 -or [BitConverter]::ToUInt16($bytes, 0) -ne 0x5A4D) {
    throw 'Expected a Windows executable.'
}
$pe = [BitConverter]::ToInt32($bytes, 60)
if ($pe -lt 64 -or $pe -gt $bytes.Length - 94 -or
    [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550 -or
    [BitConverter]::ToUInt16($bytes, $pe + 92) -ne 2) {
    throw 'Expected a GUI-subsystem executable. Rebuild with the Windows subsystem enabled.'
}
if (-not (Test-Path -LiteralPath $installDir -PathType Container)) {
    New-Item -ItemType Directory -Path $installDir > $null
}
$backup = $null
if (Test-Path -LiteralPath $target) {
    $backup = $target + '.before-startup-repair-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.bak'
    Copy-Item -LiteralPath $target -Destination $backup
}
$running = @(Get-Process -Name Vorssaint -ErrorAction SilentlyContinue | Where-Object {
    [string]::Equals($_.Path, $target, [StringComparison]::OrdinalIgnoreCase)
})
foreach ($process in $running) {
    Stop-Process -Id $process.Id
    if (-not $process.WaitForExit(10000)) { throw 'Vorssaint did not exit.' }
}
try {
    Copy-Item -LiteralPath $binary -Destination $target -Force
    if ((Get-FileHash -LiteralPath $binary).Hash -ne (Get-FileHash -LiteralPath $target).Hash) {
        throw 'Installed executable does not match the build.'
    }
    if ($EnableAutostart) {
        $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
        if (-not (Test-Path -LiteralPath $runKey)) { New-Item -Path $runKey -Force > $null }
        New-ItemProperty -LiteralPath $runKey -Name Vorssaint -PropertyType String -Value ('"' + $target + '"') -Force > $null
    }
    # Show the island itself, as Explorer/login does. The GUI subsystem prevents
    # a console; SW_HIDE can suppress the island's first native ShowWindow call.
    Start-Process -FilePath $target -WorkingDirectory ([Environment]::GetFolderPath('System'))
} catch {
    if ($null -ne $backup) {
        Copy-Item -LiteralPath $backup -Destination $target -Force
        Start-Process -FilePath $target -WorkingDirectory $installDir -WindowStyle Hidden
    }
    throw
}
Write-Output ('Installed GUI build: ' + $target)
if ($null -ne $backup) { Write-Output ('Previous executable: ' + $backup) }
if ($EnableAutostart) { Write-Output 'Start with Windows enabled for this user.' }
