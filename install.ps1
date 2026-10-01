# Native Windows installer. Runs in the caller's PowerShell session.
$ErrorActionPreference = 'Stop'
$repo = 'mel0nyrame/LinkLens'
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($env:OS -ne 'Windows_NT' -or $arch -ne 'X64') {
    throw 'Windows 预编译包仅支持 x64。macOS/Linux 请使用 install.sh。'
}
$version = $env:LINKLENS_VERSION
if (-not $version) {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
    $version = $release.tag_name
}
if ($version -notmatch '^v\d+\.\d+\.\d+$') { throw '版本应使用 v1.2.3 格式。' }
$installDir = $env:LINKLENS_INSTALL_DIR
if (-not $installDir) { $installDir = Join-Path $env:LOCALAPPDATA 'LinkLens\bin' }
$asset = "linklens-$version-x86_64-pc-windows-msvc.zip"
$base = "https://github.com/$repo/releases/download/$version"
$tempDir = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tempDir | Out-Null
try {
    $zip = Join-Path $tempDir $asset
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $zip
    $checksums = (Invoke-WebRequest -Uri "$base/SHA256SUMS").Content
    $expected = @($checksums -split "`n" | Where-Object { $_.Trim() -match ('^[a-fA-F0-9]{64}\s+' + [regex]::Escape($asset) + '$') })
    if ($expected.Count -ne 1) { throw '未找到唯一的 SHA-256 校验值。' }
    $digest = ($expected[0] -split '\s+')[0]
    if ((Get-FileHash -Algorithm SHA256 $zip).Hash -ne $digest) { throw '下载校验失败，未安装。' }
    Expand-Archive -Path $zip -DestinationPath $tempDir
    foreach ($name in @('linklens.exe', 'llens.exe')) {
        if (-not (Test-Path (Join-Path $tempDir $name))) { throw "安装包缺少 $name。" }
    }
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    Copy-Item (Join-Path $tempDir 'linklens.exe') $installDir -Force
    Copy-Item (Join-Path $tempDir 'llens.exe') $installDir -Force
    $userPath = [string][Environment]::GetEnvironmentVariable('Path', 'User')
    if ($installDir -notin ($userPath -split ';')) {
        [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $installDir), 'User')
    }
    if ($installDir -notin ($env:Path -split ';')) { $env:Path += ';' + $installDir }
    Write-Host "已安装到 $installDir，使用 linklens 或 llens 启动。"
} finally {
    Remove-Item $tempDir -Recurse -Force
}
