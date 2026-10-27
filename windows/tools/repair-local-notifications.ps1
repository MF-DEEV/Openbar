# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Local developer repair only. Registers a signed sparse package for this user
# and replaces their installed executable with an already-built release binary.
# Requires explicit permission: creates a non-exportable code-signing key and
# trusts its public certificate in CurrentUser\TrustedPeople (never Root).
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$PackagePath,
    [Parameter(Mandatory = $true)][string]$InstallDirectory,
    [Parameter(Mandatory = $true)][string]$SignToolPath
)

$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $ExecutablePath).ProviderPath
$package = (Resolve-Path -LiteralPath $PackagePath).ProviderPath
$signTool = (Resolve-Path -LiteralPath $SignToolPath).ProviderPath
$installDir = (Resolve-Path -LiteralPath $InstallDirectory).ProviderPath
$expectedDir = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Vorssaint'))
if (-not [string]::Equals($installDir.TrimEnd('\'), $expectedDir.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'This repair only targets the current user installation in LOCALAPPDATA\Vorssaint.'
}
if ([IO.Path]::GetFileName($binary) -ne 'Vorssaint.exe' -or [IO.Path]::GetExtension($package) -ne '.msix') {
    throw 'Expected a built Vorssaint.exe and a prepared .msix identity package.'
}
$target = Join-Path $installDir 'Vorssaint.exe'
if ([string]::Equals($target, $binary, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The prepared executable must be separate from the running installation.'
}

$certificateName = 'Vorssaint local notification access'
$certificate = Get-ChildItem Cert:\CurrentUser\My | Where-Object {
    $_.Subject -eq 'CN=Vorssaint' -and $_.FriendlyName -eq $certificateName -and
    $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(7)
} | Select-Object -First 1
if ($null -eq $certificate) {
    $certificate = New-SelfSignedCertificate -Type Custom -Subject 'CN=Vorssaint' `
        -FriendlyName $certificateName -CertStoreLocation 'Cert:\CurrentUser\My' `
        -KeyAlgorithm RSA -KeyLength 2048 -HashAlgorithm SHA256 -KeyUsage DigitalSignature `
        -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddYears(1) `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}

& $signTool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $package
if ($LASTEXITCODE -ne 0) { throw 'Signing the local notification package failed.' }
$publicCert = Join-Path ([IO.Path]::GetDirectoryName($package)) 'Vorssaint-local.cer'
Export-Certificate -Cert $certificate -FilePath $publicCert -Force > $null
$trustedPath = 'Cert:\CurrentUser\TrustedPeople\' + $certificate.Thumbprint
if (-not (Test-Path -LiteralPath $trustedPath)) {
    Import-Certificate -FilePath $publicCert -CertStoreLocation 'Cert:\CurrentUser\TrustedPeople' > $null
}
Write-Output ('Local package certificate: ' + $certificate.Thumbprint)

# Stop only the executable being updated; keep a copy for rollback.
$running = Get-Process Vorssaint -ErrorAction SilentlyContinue | Where-Object {
    [string]::Equals($_.Path, $target, [StringComparison]::OrdinalIgnoreCase)
}
$backup = $target + '.before-notifications-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.bak'
Copy-Item -LiteralPath $target -Destination $backup
foreach ($process in $running) { Stop-Process -Id $process.Id }
foreach ($process in $running) { Wait-Process -Id $process.Id -ErrorAction SilentlyContinue }
try {
    Copy-Item -LiteralPath $binary -Destination $target -Force
    $icon = Join-Path $PSScriptRoot '..\packaging\icon.png'
    Copy-Item -LiteralPath $icon -Destination (Join-Path $installDir 'icon.png') -Force
    Add-AppxPackage -Path $package -ExternalLocation $installDir
} catch {
    Copy-Item -LiteralPath $backup -Destination $target -Force
    Start-Process -FilePath $target -WindowStyle Hidden
    throw
}
Start-Process -FilePath $target -WindowStyle Hidden
Get-AppxPackage -Name 'Vorssaint.Island' | Select-Object Name, PackageFamilyName
Write-Output 'Updated and restarted Vorssaint. Open Notifications and allow Windows notification access when prompted.'
