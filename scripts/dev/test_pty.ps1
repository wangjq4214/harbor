#Requires -Version 7.0
# Build the PTY fixture independently of the test executable's Cargo cache layout.
param([string]$CargoPath = 'cargo')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsWindows) { throw 'The PTY test wrapper requires Windows.' }
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Push-Location $repoRoot
try {
    $probes = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    & $CargoPath build --locked -p harbor-pty --example launch_probe --message-format=json-render-diagnostics | ForEach-Object {
        $record = ConvertFrom-Json -InputObject $_
        if ($record.reason -eq 'compiler-message' -and $record.message.rendered) {
            Write-Host $record.message.rendered -NoNewline
        }
        if ($record.reason -eq 'compiler-artifact' -and $record.target.name -eq 'launch_probe' -and
            'example' -in $record.target.kind -and $record.executable) {
            [void]$probes.Add([string]$record.executable)
        }
    }
    $buildExit = $LASTEXITCODE
    if ($buildExit -ne 0) { exit $buildExit }
    if ($probes.Count -ne 1) { throw "Expected one launch_probe executable from Cargo, found $($probes.Count)." }
    $probe = @($probes)[0]
    if (-not (Test-Path -LiteralPath $probe -PathType Leaf)) { throw "Missing launch_probe executable: $probe" }
    $previous = [Environment]::GetEnvironmentVariable('HARBOR_TEST_LAUNCH_PROBE', 'Process')
    try {
        $env:HARBOR_TEST_LAUNCH_PROBE = $probe
        & (Join-Path $PSHOME 'pwsh.exe') -NoLogo -NoProfile -File (Join-Path $PSScriptRoot 'test_cargo.ps1') -CargoPath $CargoPath --locked --workspace pty:: -- --ignored --test-threads=1
        exit $LASTEXITCODE
    } finally {
        [Environment]::SetEnvironmentVariable('HARBOR_TEST_LAUNCH_PROBE', $previous, 'Process')
    }
} finally {
    Pop-Location
}
