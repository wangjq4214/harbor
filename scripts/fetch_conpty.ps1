#Requires -Version 5.1
# Restore the ignored runtime binaries from Microsoft's pinned NuGet package.
[CmdletBinding()]
param([string]$PackagePath)
$ErrorActionPreference = 'Stop'
$version = '1.24.260710001'
$packageSha256 = '175640566A3B59C4B132070EE96C2C77E5AB7EDD2E92732A5EB3610BBF63D90E'
$repoRoot = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $repoRoot "target/conpty-download/$version"
New-Item -ItemType Directory -Force $cache | Out-Null
if (-not $PackagePath) {
    $PackagePath = Join-Path $cache 'conpty.nupkg'
    if (-not (Test-Path -LiteralPath $PackagePath)) {
        $url = "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/$version/microsoft.windows.console.conpty.$version.nupkg"
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $PackagePath
    }
}
if ((Get-FileHash -LiteralPath $PackagePath -Algorithm SHA256).Hash -ne $packageSha256) {
    throw 'ConPTY package SHA256 does not match the pinned version; no runtime files were changed.'
}
# Windows PowerShell 5.1 Expand-Archive requires a .zip extension.
$zip = Join-Path $cache 'package.zip'
Copy-Item -LiteralPath $PackagePath -Destination $zip -Force
$extracted = Join-Path $cache 'package'
Expand-Archive -LiteralPath $zip -DestinationPath $extracted -Force
$destination = Join-Path $repoRoot 'third_party/conpty'
foreach ($arch in @('arm64', 'x64', 'x86')) {
    New-Item -ItemType Directory -Force (Join-Path $destination $arch) | Out-Null
    Copy-Item -LiteralPath "$extracted/runtimes/win-$arch/native/conpty.dll" -Destination "$destination/$arch/conpty.dll" -Force
    Copy-Item -LiteralPath "$extracted/build/native/runtimes/$arch/OpenConsole.exe" -Destination "$destination/$arch/OpenConsole.exe" -Force
}
Copy-Item -LiteralPath "$extracted/inc/conpty.h" -Destination "$destination/conpty.h" -Force
Set-Content -LiteralPath "$destination/VERSION" -Value $version -Encoding Ascii
$hashes = foreach ($arch in @('arm64', 'x64', 'x86')) {
    foreach ($file in @('conpty.dll', 'OpenConsole.exe')) {
        $hash = (Get-FileHash -LiteralPath "$destination/$arch/$file" -Algorithm SHA256).Hash
        "$hash  $arch/$file"
    }
}
Set-Content -LiteralPath "$destination/SHA256SUMS" -Value $hashes -Encoding Ascii
Write-Output "Restored Microsoft ConPTY $version; rebuild Harbor to stage the runtime."
