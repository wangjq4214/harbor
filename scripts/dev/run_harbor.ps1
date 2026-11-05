#Requires -Version 7.0
<#
.SYNOPSIS
Build and run Harbor with either upstream Cargo or huacnlee/cargo.
.DESCRIPTION
Stage the complete pinned ConPTY runtime beside Cargo's reported executable on
every invocation, including fresh/cache-hit builds. No shared-cache layout is
assumed. CI can continue using upstream Cargo directly.
.PARAMETER BuildOnly
Stage the runtime and return the executable path without starting Harbor.
.PARAMETER CargoPath
Cargo command or executable path. Defaults to the Cargo already on PATH.
#>
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$BuildOnly,
    [string]$CargoPath = 'cargo',
    [string[]]$AppArgs = @()
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Harbor development launch requires Windows.' }
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

function Get-ImageArchitecture([string]$Path) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE image: $Path" }
        $reader.BaseStream.Position = 0x3C
        $offset = $reader.ReadUInt32()
        if ($offset -gt $reader.BaseStream.Length - 6) { throw "Invalid PE header: $Path" }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Not a PE image: $Path" }
        switch ($reader.ReadUInt16()) {
            0x8664 { return 'x64' }
            0xAA64 { return 'arm64' }
            0x014C { return 'x86' }
            default { throw "Unsupported PE machine in $Path" }
        }
    } finally {
        $reader.Dispose()
    }
}

Push-Location $repoRoot
try {
    $version = & $CargoPath --version
    if ($LASTEXITCODE -ne 0) { throw 'Could not identify Cargo.' }
    $flavor = if ("$version" -match 'huacnlee/cargo') { 'huacnlee/cargo (shared build cache)' } else { 'standard/other Cargo' }
    Write-Host "Using $flavor`: $version"
    $buildArgs = @('build', '--locked', '--package', 'harbor', '--bin', 'harbor', '--message-format=json-render-diagnostics')
    if ($Release) { $buildArgs += '--release' }
    $executables = [Collections.Generic.HashSet[string]]::new()
    & $CargoPath @buildArgs | ForEach-Object {
        $record = ConvertFrom-Json -InputObject $_
        if ($record.reason -eq 'compiler-message' -and $record.message.rendered) {
            Write-Host $record.message.rendered -NoNewline
        }
        if ($record.reason -eq 'compiler-artifact' -and
            $record.target.name -eq 'harbor' -and 'bin' -in $record.target.kind -and
            $record.manifest_path -eq (Join-Path $repoRoot 'Cargo.toml') -and
            $record.executable) {
            [void]$executables.Add($record.executable)
        }
    }
    if ($LASTEXITCODE -ne 0) { throw 'Harbor build failed; nothing was staged or launched.' }
    if ($executables.Count -ne 1) { throw 'Cargo did not report exactly one Harbor executable.' }
    $executable = @($executables)[0]
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "Missing executable: $executable" }
    $arch = Get-ImageArchitecture $executable
    $source = Join-Path $repoRoot 'third_party/conpty'
    $files = [ordered]@{ 'conpty.dll' = "$arch/conpty.dll" }
    foreach ($hostArch in @('x64', 'arm64', 'x86')) {
        $files["$hostArch/OpenConsole.exe"] = "$hostArch/OpenConsole.exe"
    }
    $files['LICENSE'] = 'LICENSE'
    $files['VERSION'] = 'VERSION'
    # Validate the entire source before touching the destination. Do not fetch
    # dependencies or change the user's toolchain/configuration automatically.
    foreach ($relative in $files.Values) {
        if (-not (Test-Path -LiteralPath (Join-Path $source $relative) -PathType Leaf)) {
            throw "Missing bundled ConPTY source: $relative. Run ./scripts/setup/fetch_conpty.ps1 first."
        }
    }
    $runtime = Join-Path (Split-Path -Parent $executable) 'conpty'
    foreach ($entry in $files.GetEnumerator()) {
        $from = Join-Path $source $entry.Value
        $to = Join-Path $runtime $entry.Key
        if ((Test-Path -LiteralPath $to -PathType Leaf) -and
            (Get-FileHash -LiteralPath $from).Hash -eq (Get-FileHash -LiteralPath $to).Hash) {
            continue
        }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to -Force
    }
    Write-Host "ConPTY ($arch) ready: $runtime"
    if ($BuildOnly) {
        Write-Output $executable
    } else {
        & $executable @AppArgs
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}
