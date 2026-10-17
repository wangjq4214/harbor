#Requires -Version 5.1
# Consumes the shared packager's isolated payload; WiX harvests the same contents.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PayloadDirectory,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$OutputPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'windows_package_version.ps1')
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$authoring = Join-Path $root 'installer/windows'
$toolDirectory = Join-Path $root 'target/wix'
$work = $null
$previousRollForward = $env:DOTNET_ROLL_FORWARD
try {
    if (-not [IO.Path]::IsPathRooted($PayloadDirectory) -or -not [IO.Path]::IsPathRooted($OutputPath)) {
        throw 'PayloadDirectory and OutputPath must be absolute paths.'
    }
    Assert-WindowsPackageVersion $Version
    $payload = (Resolve-Path -LiteralPath $PayloadDirectory).Path
    if (-not (Test-Path -LiteralPath (Join-Path $payload 'harbor.exe') -PathType Leaf)) {
        throw 'Validated payload must contain harbor.exe.'
    }
    if (Test-Path -LiteralPath (Join-Path $payload 'harbor.portable')) {
        throw 'Portable marker harbor.portable must not be included in the MSI payload.'
    }
    $out = [IO.Path]::GetFullPath($OutputPath)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $out) | Out-Null
    $work = Join-Path ([IO.Path]::GetTempPath()) ('harbor-msi-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $work | Out-Null
    $license = Join-Path $work 'License.rtf'
    $licenseText = Get-Content -LiteralPath (Join-Path $payload 'LICENSE') -Raw
    $escaped = $licenseText.Replace('\', '\\').Replace('{', '\{').Replace('}', '\}')
    $escaped = $escaped -replace '\r?\n', '\par '
    [IO.File]::WriteAllText($license, "{\rtf1\ansi\deff0{\fonttbl{\f0\fswiss Arial;}}\f0\fs18 $escaped}", [Text.Encoding]::ASCII)

    # Both the dotnet tool and extension caches live under ignored target/, not globally.
    $configDirectory = Join-Path $toolDirectory '.config'
    New-Item -ItemType Directory -Force -Path $configDirectory | Out-Null
    Copy-Item -LiteralPath (Join-Path $authoring '.config/dotnet-tools.json') -Destination $configDirectory -Force
    $env:DOTNET_ROLL_FORWARD = 'Major'
    Push-Location $toolDirectory
    try {
        & dotnet tool restore
        if ($LASTEXITCODE -ne 0) { throw 'Could not restore pinned WiX 5.0.2. Install the .NET 8+ SDK (not only its runtime) and allow NuGet access.' }
        & dotnet tool run wix -- extension add WixToolset.UI.wixext/5.0.2
        if ($LASTEXITCODE -ne 0) { throw 'Could not restore pinned WiX UI extension 5.0.2.' }
        & dotnet tool run wix -- build (Join-Path $authoring 'Harbor.wxs') -ext WixToolset.UI.wixext/5.0.2 -d "ProductVersion=$Version" -d "PayloadDirectory=$payload" -bindvariable "WixUILicenseRtf=$license" -arch x64 -culture en-US -pdbtype none -o $out
        if ($LASTEXITCODE -ne 0) { throw "WiX failed to build MSI (exit code $LASTEXITCODE)." }
        & dotnet tool run wix -- msi validate $out
        if ($LASTEXITCODE -ne 0) { throw "MSI consistency validation failed (exit code $LASTEXITCODE)." }
    } finally { Pop-Location }
    if (-not (Test-Path -LiteralPath $out -PathType Leaf)) { throw "WiX reported success but produced no MSI at $out" }
    Write-Output $out
} finally {
    $env:DOTNET_ROLL_FORWARD = $previousRollForward
    if ($work -and (Test-Path -LiteralPath $work)) { Remove-Item -LiteralPath $work -Recurse -Force }
}
