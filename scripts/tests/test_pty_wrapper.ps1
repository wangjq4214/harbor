#Requires -Version 7.0
# Exercise the PTY entry point with isolated fake Cargo and test wrapper processes.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'These checks require Windows and PowerShell 7.' }
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$temp = Join-Path ([IO.Path]::GetTempPath()) ('harbor-pty-wrapper-' + [guid]::NewGuid().ToString('N'))
$repo = Join-Path $temp 'fixture repo'
$dev = Join-Path $repo 'scripts/dev'
$pwsh = Join-Path $PSHOME 'pwsh.exe'
function Assert-That([bool]$condition, [string]$message) { if (-not $condition) { throw $message } }
try {
    New-Item -ItemType Directory -Force -Path $dev | Out-Null
    Copy-Item (Join-Path $root 'scripts/dev/test_pty.ps1') $dev
    $probe = Join-Path $temp 'unrelated cache/example output/probe with spaces.exe'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $probe) | Out-Null
    [IO.File]::WriteAllText($probe, 'fixture')
    $config = Join-Path $dev 'config.json'
    $log = Join-Path $dev 'calls.jsonl'
    $cargo = Join-Path $dev 'fake cargo.ps1'
    [IO.File]::WriteAllText($cargo, @'
$c = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'config.json') | ConvertFrom-Json
Add-Content (Join-Path $PSScriptRoot 'calls.jsonl') (ConvertTo-Json -Compress @{ kind = 'build'; args = @($args); cwd = (Get-Location).Path })
foreach ($artifact in $c.artifacts) {
    ConvertTo-Json -Compress -Depth 10 @{ reason = 'compiler-artifact'; target = @{ name = $artifact.name; kind = @($artifact.kind) }; executable = $artifact.path; fresh = $true }
}
$global:LASTEXITCODE = [int]$c.buildExit
'@)
    [IO.File]::WriteAllText((Join-Path $dev 'test_cargo.ps1'), @'
Add-Content (Join-Path $PSScriptRoot 'calls.jsonl') (ConvertTo-Json -Compress @{ kind = 'test'; args = @($args); probe = $env:HARBOR_TEST_LAUNCH_PROBE; cwd = (Get-Location).Path })
$c = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'config.json') | ConvertFrom-Json
exit [int]$c.testExit
'@)
    $artifact = @{ name = 'launch_probe'; kind = 'example'; path = $probe }
    $state = @{ artifacts = @($artifact); buildExit = 0; testExit = 0 }
    function Invoke-Fixture {
        [IO.File]::WriteAllText($config, (ConvertTo-Json -InputObject $state -Depth 10))
        Remove-Item -LiteralPath $log -ErrorAction SilentlyContinue
        $out = @(& $pwsh -NoLogo -NoProfile -File (Join-Path $dev 'test_pty.ps1') -CargoPath $cargo 2>&1)
        $code = $LASTEXITCODE
        $calls = @(if (Test-Path $log) { Get-Content $log | ForEach-Object { ConvertFrom-Json $_ } })
        return @{ code = $code; calls = $calls; output = ($out | Out-String) }
    }
    $r = Invoke-Fixture
    Assert-That ($r.code -eq 0 -and $r.calls.Count -eq 2) "Fixture failed: $($r.output)"
    Assert-That ($r.calls[1].probe -ceq $probe) 'The reported executable was not passed unchanged.'
    Assert-That ($r.calls[0].cwd -eq $repo -and $r.calls[1].cwd -eq $repo) 'Wrong working directory.'
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.calls[0].args)) -ceq '["build","--locked","-p","harbor-pty","--example","launch_probe","--message-format=json-render-diagnostics"]') 'Wrong build arguments.'
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.calls[1].args)) -ceq (ConvertTo-Json -Compress -InputObject @('-CargoPath', $cargo, '--locked', '--workspace', 'pty::', '--', '--ignored', '--test-threads=1'))) 'Wrong test arguments.'
    $state.buildExit = 19; $r = Invoke-Fixture
    Assert-That ($r.code -eq 19 -and $r.calls.Count -eq 1) 'Build failure must prevent test execution.'
    $state.buildExit = 0; $state.testExit = 23; $r = Invoke-Fixture
    Assert-That ($r.code -eq 23 -and $r.calls.Count -eq 2) 'Test failure must propagate.'
    $state.testExit = 0
    foreach ($artifacts in @(@(), @(@{ name = 'launch_probe'; kind = 'bin'; path = $probe }), @(@{ name = 'launch_probe'; kind = 'example'; path = (Join-Path $temp 'missing.exe') }), @($artifact, @{ name = 'launch_probe'; kind = 'example'; path = (Join-Path $temp 'second.exe') }))) {
        $state.artifacts = $artifacts; $r = Invoke-Fixture
        Assert-That ($r.code -ne 0 -and $r.calls.Count -eq 1) 'Missing, wrong-kind, nonexistent or ambiguous artifacts must prevent test execution.'
    }
    Write-Output 'PASS: PTY probe artifact selection, path forwarding and failure handling.'
} finally {
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
