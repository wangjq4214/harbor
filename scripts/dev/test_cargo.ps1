#Requires -Version 7.0
<#
.SYNOPSIS
Run Cargo tests after staging ConPTY beside Cargo's reported Windows executables.
.DESCRIPTION
Pass the arguments normally following `cargo test`, including a test filter and
optional `--` harness arguments. Put -CargoPath before Cargo arguments to select a different Cargo executable.
#>
# Parse explicitly: PowerShell's advanced parameter binder treats Cargo's -p as
# an ambiguous common parameter before ValueFromRemainingArguments can see it.
$TestArgs = @($args)
$CargoPath = 'cargo'
if ($TestArgs.Count -gt 0 -and $TestArgs[0] -eq '-CargoPath') {
    if ($TestArgs.Count -lt 2) { throw '-CargoPath requires a value.' }
    $CargoPath = $TestArgs[1]
    $TestArgs = @($TestArgs | Select-Object -Skip 2)
}
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Preserve native Cargo exit codes even if the caller opted into native errors.
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows) { throw 'The Cargo test staging wrapper requires Windows.' }
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
. (Join-Path $PSScriptRoot 'conpty_staging.ps1')

$separator = [Array]::IndexOf($TestArgs, '--')
$cargoArgs = @(if ($separator -lt 0) { $TestArgs } elseif ($separator -gt 0) { $TestArgs[0..($separator - 1)] })
foreach ($arg in $cargoArgs) {
    if ($arg -eq '--doc' -or $arg -eq '--no-run' -or $arg -match '^--message-format(?:=|$)') {
        throw "Unsupported wrapper argument: $arg"
    }
}
Push-Location $repoRoot
# The Cargo fork's shared workspace units break nested trybuild builds. Upstream
# Cargo ignores this variable; apply it to both invocations and their children.
$shareWorkspaceUnits = [Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process')
try {
    $executables = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    [Environment]::SetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', '0', 'Process')
    & $CargoPath test --no-run --message-format=json-render-diagnostics @cargoArgs | ForEach-Object {
        $record = ConvertFrom-Json -InputObject $_
        if ($record.reason -eq 'compiler-message' -and $record.message.rendered) {
            Write-Host $record.message.rendered -NoNewline
        }
        if ($record.reason -eq 'compiler-artifact' -and $record.executable) {
            [void]$executables.Add([string]$record.executable)
        }
    }
    $buildExit = $LASTEXITCODE
    if ($buildExit -ne 0) { exit $buildExit }

    # A single directory cannot hold DLLs for two architectures. Check every
    # executable before staging so invalid artifacts never lead to a test run.
    $directories = @{}
    foreach ($executable in $executables) {
        if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw "Missing executable: $executable" }
        $arch = Get-ImageArchitecture $executable
        $directory = Split-Path -Parent $executable
        if ($directories.ContainsKey($directory) -and $directories[$directory] -ne $arch) {
            throw "Conflicting executable architectures in $directory"
        }
        $directories[$directory] = $arch
    }
    foreach ($executable in $executables) { Stage-Conpty $executable $repoRoot }
    & $CargoPath test @TestArgs
    exit $LASTEXITCODE
} finally {
    [Environment]::SetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', $shareWorkspaceUnits, 'Process')
    Pop-Location
}
