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

. (Join-Path $PSScriptRoot 'conpty_staging.ps1')

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
    Stage-Conpty $executable $repoRoot
    if ($BuildOnly) {
        Write-Output $executable
    } else {
        & $executable @AppArgs
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}
