# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint

# Builds the signed sparse package that gives the installed Vorssaint.exe its
# package identity (notifications, calendar), plus the public certificate the
# installer trusts. Runs on Windows with the Windows SDK (makeappx, signtool).
#
#   $env:VORSSAINT_SIGNING_PFX      = 'C:\keys\vorssaint-signing.pfx'   # outside the repo
#   $env:VORSSAINT_SIGNING_PASSWORD = '...'
#   .\build-sparse.ps1 [-NewCertificate] [-Revision 3]
#
# -NewCertificate makes the self-signed "CN=Vorssaint" code-signing key once
# and writes it to that PFX; keep the PFX: every later package must be signed
# by the same certificate, or installs that trusted the old one fail.
# Writes packaging\out\Vorssaint.msix and Vorssaint.cer, which
# src-tauri\tauri.identity.conf.json bundles into the installer.
[CmdletBinding()]
param(
    [string]$OutDirectory = (Join-Path $PSScriptRoot '..\..\packaging\out'),
    # The package version is the app's x.y.z plus this fourth part, so a
    # rebuilt package of one release can still replace the last one.
    [ValidateRange(0, 65535)][int]$Revision = 0,
    [switch]$NewCertificate
)

$ErrorActionPreference = 'Stop'
$pfxPath = $env:VORSSAINT_SIGNING_PFX
$password = $env:VORSSAINT_SIGNING_PASSWORD
if ([string]::IsNullOrEmpty($pfxPath) -or [string]::IsNullOrEmpty($password)) {
    throw 'Set VORSSAINT_SIGNING_PFX (a path outside the repository) and VORSSAINT_SIGNING_PASSWORD.'
}
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).ProviderPath
if ([IO.Path]::GetFullPath($pfxPath).StartsWith($repo, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Keep the signing PFX outside the repository.'
}

$manifestPath = Join-Path $repo 'packaging\AppxManifest.xml'
[xml]$manifest = Get-Content -LiteralPath $manifestPath -Raw
$publisher = $manifest.Package.Identity.Publisher

if ($NewCertificate) {
    if (Test-Path -LiteralPath $pfxPath) { throw "$pfxPath exists already; reuse it instead of making a new key." }
    $created = New-SelfSignedCertificate -Type CodeSigningCert -Subject $publisher `
        -FriendlyName 'Vorssaint package signing' -CertStoreLocation 'Cert:\CurrentUser\My' `
        -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy Exportable `
        -NotAfter (Get-Date).AddYears(10) -TextExtension @('2.5.29.19={text}')
    try {
        $secure = ConvertTo-SecureString -String $password -AsPlainText -Force
        Export-PfxCertificate -Cert $created -FilePath $pfxPath -Password $secure > $null
    } finally {
        Remove-Item -LiteralPath ('Cert:\CurrentUser\My\' + $created.Thumbprint) -DeleteKey
    }
    Write-Output "New signing certificate: $pfxPath ($($created.Thumbprint))"
}

# Windows only installs the package if its signer is the manifest's Publisher.
$certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 -ArgumentList $pfxPath, $password
if ($certificate.Subject -ne $publisher) {
    throw "The certificate is for '$($certificate.Subject)', the manifest's Publisher is '$publisher'."
}

function Find-SdkTool([string]$name) {
    if ($env:WINDOWS_SDK_BIN) {
        $path = Join-Path $env:WINDOWS_SDK_BIN $name
        if (Test-Path -LiteralPath $path) { return $path }
    }
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $found = Get-ChildItem -LiteralPath $kits -Directory -Filter '10.*' -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { foreach ($a in @($arch, 'x64', 'x86')) { Join-Path $_.FullName "$a\$name" } } |
        Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $found) { throw "$name not found. Install the Windows SDK, or set WINDOWS_SDK_BIN." }
    return $found
}
$makeAppx = Find-SdkTool 'makeappx.exe'
$signTool = Find-SdkTool 'signtool.exe'

$config = Get-Content -LiteralPath (Join-Path $repo 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$version = '{0}.{1}' -f $config.version, $Revision
$manifest.Package.Identity.Version = $version

$staging = Join-Path ([IO.Path]::GetTempPath()) ('vorssaint-identity-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $staging > $null
New-Item -ItemType Directory -Force -Path $OutDirectory > $null
$out = (Resolve-Path -LiteralPath $OutDirectory).ProviderPath
$package = Join-Path $out 'Vorssaint.msix'
$publicCertificate = Join-Path $out 'Vorssaint.cer'
try {
    $manifest.Save((Join-Path $staging 'AppxManifest.xml'))
    Copy-Item -LiteralPath (Join-Path $repo 'packaging\icon.png') -Destination $staging
    # /nv: a sparse package's executable lives outside it, in the install folder.
    & $makeAppx pack /d $staging /p $package /nv /o
    if ($LASTEXITCODE -ne 0) { throw 'makeappx failed.' }
    & $signTool sign /fd SHA256 /f $pfxPath /p $password $package
    if ($LASTEXITCODE -ne 0) { throw 'signtool failed.' }
} finally {
    Remove-Item -LiteralPath $staging -Recurse -Force
}
[IO.File]::WriteAllBytes($publicCertificate, $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert))
Write-Output "Package $version signed by $($certificate.Subject) ($($certificate.Thumbprint))"
Write-Output $package
Write-Output $publicCertificate
