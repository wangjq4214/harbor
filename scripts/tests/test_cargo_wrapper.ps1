#Requires -Version 7.0
# Offline isolated fake-Cargo checks; no Cargo, fixture PE, GUI or network runs.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'These checks require Windows and PowerShell 7.' }
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$temp = Join-Path ([IO.Path]::GetTempPath()) ('harbor-test-wrapper-' + [guid]::NewGuid().ToString('N'))
$repo = Join-Path $temp 'fixture repo'
$tools = Join-Path $temp 'fake tools'
$pwsh = Join-Path $PSHOME 'pwsh.exe'
function Assert-That([bool]$condition, [string]$message) { if (-not $condition) { throw $message } }
function Pe([string]$path, [uint16]$machine) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    $bytes = [byte[]]::new(512)
    $bytes[0] = 0x4d; $bytes[1] = 0x5a
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes, 0x80)
    [BitConverter]::GetBytes($machine).CopyTo($bytes, 0x84)
    [IO.File]::WriteAllBytes($path, $bytes)
}
function Json([string]$path, $value) { [IO.File]::WriteAllText($path, (ConvertTo-Json -InputObject $value -Depth 20)) }
$originalShare = [Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process')
try {
    New-Item -ItemType Directory -Force -Path (Join-Path $repo 'scripts/dev'), $tools | Out-Null
    foreach ($name in @('test_cargo.ps1', 'conpty_staging.ps1')) {
        Copy-Item -LiteralPath (Join-Path $root "scripts/dev/$name") -Destination (Join-Path $repo "scripts/dev/$name")
    }
    $source = Join-Path $repo 'third_party/conpty'
    foreach ($arch in @('x64', 'arm64', 'x86')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $source $arch) | Out-Null
        [IO.File]::WriteAllText((Join-Path $source "$arch/conpty.dll"), "dll $arch")
        [IO.File]::WriteAllText((Join-Path $source "$arch/OpenConsole.exe"), "host $arch")
    }
    foreach ($name in @('LICENSE', 'VERSION')) { [IO.File]::WriteAllText((Join-Path $source $name), $name) }
    $one = Join-Path $temp 'shared cache/one/test one.exe'
    $two = Join-Path $temp 'other cache/deps/test-two.exe'
    Pe $one 0x8664; Pe $two 0xAA64
    $config = Join-Path $tools 'config.json'
    $calls = Join-Path $tools 'calls.jsonl'
    $cargo = Join-Path $tools 'fake cargo.ps1'
    $wrapper = Join-Path $repo 'scripts/dev/test_cargo.ps1'
    [IO.File]::WriteAllText($cargo, @'
$c = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'config.json') | ConvertFrom-Json
$nestedShare = & (Join-Path $PSHOME 'pwsh.exe') -NoLogo -NoProfile -Command '[Environment]::GetEnvironmentVariable("CARGO_SHARE_WORKSPACE_UNITS", "Process")'
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'calls.jsonl') -Value (ConvertTo-Json -Compress -InputObject @{ args = @($args); cwd = (Get-Location).Path; share = [Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process'); nestedShare = $nestedShare })
if ('--no-run' -in $args) {
    ConvertTo-Json -Compress -Depth 10 -InputObject @{ reason = 'compiler-message'; message = @{ rendered = 'fixture diagnostic' } }
    ConvertTo-Json -Compress -Depth 10 -InputObject @{ reason = 'compiler-artifact'; executable = $null; fresh = $false }
    foreach ($path in $c.executables) {
        ConvertTo-Json -Compress -Depth 10 -InputObject @{ reason = 'compiler-artifact'; executable = $path; fresh = $true }
    }
    $global:LASTEXITCODE = [int]$c.buildExit
} else {
    foreach ($path in $c.executables) {
        $dir = Join-Path (Split-Path -Parent $path) 'conpty'
        foreach ($name in @('conpty.dll', 'x64/OpenConsole.exe', 'arm64/OpenConsole.exe', 'x86/OpenConsole.exe', 'LICENSE', 'VERSION')) {
            if (-not (Test-Path -LiteralPath (Join-Path $dir $name) -PathType Leaf)) { throw "Test run before staging: $dir/$name" }
        }
    }
    $global:LASTEXITCODE = [int]$c.runExit
}
'@)
    $driver = Join-Path $tools 'driver.ps1'
    $restored = Join-Path $tools 'restored.json'
    $argsFile = Join-Path $tools 'args.json'
    [IO.File]::WriteAllText($driver, @'
param($wrapper, $cargo, $restored, $argsFile)
$testArgs = @(Get-Content -Raw -LiteralPath $argsFile | ConvertFrom-Json)
try {
    & $wrapper -CargoPath $cargo @testArgs
} finally {
    [IO.File]::WriteAllText($restored, (ConvertTo-Json -Compress -InputObject @{
        share = [Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process')
    }))
}
exit $LASTEXITCODE
'@)
    $state = @{ executables = @($one, $two, $one); buildExit = 0; runExit = 0 }
    $argsToTest = @('--locked', '-p', 'harbor-pty', '--features', 'alpha,beta', '--target', 'x86_64-pc-windows-msvc', 'pty::', '--', '--ignored', '--test-threads=1')
    function Invoke-Wrapper {
        Json $config $state
        Json $argsFile @($argsToTest)
        Remove-Item -LiteralPath $calls, $restored -ErrorAction SilentlyContinue
        $before = [Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process')
        Push-Location $temp
        try { $out = @(& $pwsh -NoLogo -NoProfile -File $driver $wrapper $cargo $restored $argsFile 2>&1); $code = $LASTEXITCODE }
        finally { Pop-Location }
        Assert-That ([Environment]::GetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', 'Process') -ceq $before) 'Parent environment changed.'
        Assert-That (Test-Path -LiteralPath $restored) "Wrapper did not restore environment in driver: $($out | Out-String)"
        $after = (Get-Content -Raw -LiteralPath $restored | ConvertFrom-Json).share
        Assert-That ($after -ceq $before) "Wrapper did not restore environment: before=$before after=$after"
        $log = @(if (Test-Path -LiteralPath $calls) { Get-Content -LiteralPath $calls | ForEach-Object { ConvertFrom-Json $_ } })
        foreach ($call in $log) {
            Assert-That ($call.share -ceq '0' -and $call.nestedShare -ceq '0') 'Cargo or nested process did not receive disabled sharing.'
        }
        return @{ code = $code; log = $log; output = ($out | Out-String) }
    }
    [Environment]::SetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', $null, 'Process')
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0) "Wrapper failed: $($r.output)"
    Assert-That ($r.log.Count -eq 2) 'Expected prebuild and test.'
    $expectedBuild = @('test', '--no-run', '--message-format=json-render-diagnostics') + $argsToTest[0..7]
    $expectedRun = @('test') + $argsToTest
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.log[0].args)) -ceq (ConvertTo-Json -Compress -InputObject $expectedBuild)) "Bad build args: $($r.log[0].args)"
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.log[1].args)) -ceq (ConvertTo-Json -Compress -InputObject $expectedRun)) "Bad run args: $($r.log[1].args)"
    foreach ($call in $r.log) { Assert-That ($call.cwd -eq $repo) 'Wrong Cargo working directory.' }
    [Environment]::SetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', '1', 'Process')
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0 -and $r.log.Count -eq 2) "Explicit sharing override failed: $($r.output)"
    foreach ($pair in @(@($one, 'x64'), @($two, 'arm64'))) {
        $dest = Join-Path (Split-Path -Parent $pair[0]) 'conpty/conpty.dll'
        Assert-That ((Get-FileHash $dest).Hash -eq (Get-FileHash (Join-Path $source "$($pair[1])/conpty.dll")).Hash) 'Wrong architecture DLL.'
    }
    $runtime = Join-Path (Split-Path -Parent $one) 'conpty'
    $stamp = [datetime]::new(2001, 2, 3, 4, 5, 6, [DateTimeKind]::Utc)
    (Get-Item (Join-Path $runtime 'LICENSE')).LastWriteTimeUtc = $stamp
    [IO.File]::WriteAllText((Join-Path $runtime 'conpty.dll'), 'corrupt')
    Remove-Item (Join-Path $runtime 'x86/OpenConsole.exe')
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0) "Repair failed: $($r.output)"
    Assert-That ((Get-Item (Join-Path $runtime 'LICENSE')).LastWriteTimeUtc -eq $stamp) 'Identical file recopied.'
    Assert-That ((Get-FileHash (Join-Path $runtime 'conpty.dll')).Hash -eq (Get-FileHash (Join-Path $source 'x64/conpty.dll')).Hash) 'Corrupt DLL not repaired.'
    $state.buildExit = 17; $r = Invoke-Wrapper
    Assert-That ($r.code -eq 17 -and $r.log.Count -eq 1) 'Build failure must propagate without running tests.'
    $state.executables = @(); $r = Invoke-Wrapper
    Assert-That ($r.code -eq 17 -and $r.log.Count -eq 1) 'Build failure without artifacts must not run tests.'
    $state.executables = @($one, $two, $one)
    $state.buildExit = 0; $state.runExit = 23; $r = Invoke-Wrapper
    Assert-That ($r.code -eq 23 -and $r.log.Count -eq 2) 'Test failure must propagate.'
    $state.runExit = 0
    Remove-Item (Join-Path $source 'x86/OpenConsole.exe')
    $r = Invoke-Wrapper
    Assert-That ($r.code -ne 0 -and $r.log.Count -eq 1) 'Missing source must prevent test execution.'
    [IO.File]::WriteAllText((Join-Path $source 'x86/OpenConsole.exe'), 'restored')
    Pe $two 0x01c4
    $r = Invoke-Wrapper
    Assert-That ($r.code -ne 0 -and $r.log.Count -eq 1) 'Invalid PE must prevent test execution.'
    Pe $two 0xAA64
    Remove-Item -LiteralPath $two
    $r = Invoke-Wrapper
    Assert-That ($r.code -ne 0 -and $r.log.Count -eq 1) 'Missing artifact must prevent test execution.'
    Pe $two 0xAA64
    $conflict = Join-Path (Split-Path -Parent $one) 'other-arch.exe'
    Pe $conflict 0x014C
    $state.executables = @($one, $conflict)
    $r = Invoke-Wrapper
    Assert-That ($r.code -ne 0 -and $r.log.Count -eq 1) 'Conflicting directory architectures must prevent test execution.'
    $state.executables = @($one, $two)
    $argsToTest = @('--', '--ignored')
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0 -and $r.log.Count -eq 2) "Harness-only arguments failed: $($r.output)"
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.log[0].args)) -ceq '["test","--no-run","--message-format=json-render-diagnostics"]') "Harness args leaked into prebuild: $(ConvertTo-Json -Compress -InputObject @($r.log[0].args))"
    $argsToTest = @()
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0 -and $r.log.Count -eq 2) "No-argument test failed: $($r.output)"
    $argsToTest = @('-p', 'harbor-parser', '')
    $r = Invoke-Wrapper
    Assert-That ($r.code -eq 0 -and $r.log.Count -eq 2) "Empty filter failed: $($r.output)"
    Assert-That ((ConvertTo-Json -Compress -InputObject @($r.log[1].args)) -ceq '["test","-p","harbor-parser",""]') "Empty filter was not preserved: $(ConvertTo-Json -Compress -InputObject @($r.log[1].args))"
    foreach ($unsupported in @('--doc', '--no-run', '--message-format=json')) {
        $argsToTest = @($unsupported)
        $r = Invoke-Wrapper
        Assert-That ($r.code -ne 0 -and $r.log.Count -eq 0) "Unsupported argument $unsupported was not rejected."
    }
    Write-Output 'PASS: isolated fake Cargo staging, arguments, cache repairs and failures.'
} finally {
    [Environment]::SetEnvironmentVariable('CARGO_SHARE_WORKSPACE_UNITS', $originalShare, 'Process')
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
