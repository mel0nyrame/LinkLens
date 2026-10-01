# Test the native installer with local release fixtures and real archive/hash operations.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$oldUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$oldPath = $env:Path
$oldDir = $env:LINKLENS_INSTALL_DIR
$oldVersion = $env:LINKLENS_VERSION
New-Item -ItemType Directory -Path $tempRoot | Out-Null
$script:fixtureDir = $tempRoot
$script:badChecksum = $false
function Invoke-RestMethod { param($Uri) return @{tag_name = 'v1.2.3'} }
function Invoke-WebRequest {
    param($Uri, $OutFile)
    if ($OutFile) {
        Copy-Item (Join-Path $script:fixtureDir 'fixture.zip') $OutFile
    } else {
        $hash = (Get-FileHash (Join-Path $script:fixtureDir 'fixture.zip') -Algorithm SHA256).Hash.ToLower()
        if ($script:badChecksum) { $hash = '0' * 64 }
        return @{Content = "$hash  linklens-v1.2.3-x86_64-pc-windows-msvc.zip`n"}
    }
}
try {
    $source = Join-Path $tempRoot 'source'
    New-Item -ItemType Directory -Path $source | Out-Null
    Set-Content (Join-Path $source 'linklens.exe') 'fixture executable'
    Set-Content (Join-Path $source 'llens.exe') 'fixture executable'
    Compress-Archive -Path (Join-Path $source '*') -DestinationPath (Join-Path $tempRoot 'fixture.zip')
    $env:LINKLENS_INSTALL_DIR = Join-Path $tempRoot 'install space'
    $env:LINKLENS_VERSION = ''
    & (Join-Path $root 'install.ps1')
    foreach ($name in @('linklens.exe', 'llens.exe')) {
        if (-not (Test-Path (Join-Path $env:LINKLENS_INSTALL_DIR $name))) { throw "Missing $name" }
    }
    $script:badChecksum = $true
    $env:LINKLENS_INSTALL_DIR = Join-Path $tempRoot 'rejected'
    $failed = $false
    try { & (Join-Path $root 'install.ps1') } catch { $failed = $true }
    if (-not $failed -or (Test-Path $env:LINKLENS_INSTALL_DIR)) { throw 'Invalid checksum was installed' }
    Write-Host 'Windows installer tests passed.'
} finally {
    [Environment]::SetEnvironmentVariable('Path', $oldUserPath, 'User')
    $env:Path = $oldPath
    $env:LINKLENS_INSTALL_DIR = $oldDir
    $env:LINKLENS_VERSION = $oldVersion
    Remove-Item $tempRoot -Recurse -Force
}
