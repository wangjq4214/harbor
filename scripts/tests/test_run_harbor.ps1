#Requires -Version 7.0
<#
.SYNOPSIS
Offline regression checks for scripts/dev/run_harbor.ps1 (Windows/PS7).
.DESCRIPTION
Copies only the launcher into disposable fixture repositories. Fake Cargo emits
JSON and sets global LASTEXITCODE like a native command; no builds, GUI or network
are used. Header-only PE fixtures must never run: child-process launch traps turn
any attempted application launch into a recorded test failure.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'These launcher regression checks require Windows and PowerShell 7.' }
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$launcher = Join-Path $repoRoot 'scripts/dev/run_harbor.ps1'
if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
    throw "Launcher not available; tests not run: $launcher"
}
$pwsh = Join-Path $PSHOME 'pwsh.exe'
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('harbor-run-tests-' + [Guid]::NewGuid().ToString('N'))

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Write-Json([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 30))
}

function New-PeImage([string]$Path, [uint16]$Machine) {
    New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force | Out-Null
    # DOS header, PE signature, COFF machine, and a minimal optional header.
    # Deliberately no code/entry point: these are parser inputs, not applications.
    $bytes = [byte[]]::new(512)
    $bytes[0] = 0x4D; $bytes[1] = 0x5A
    [BitConverter]::GetBytes([uint32]0x80).CopyTo($bytes, 0x3C)
    [BitConverter]::GetBytes([uint32]0x00004550).CopyTo($bytes, 0x80)
    [BitConverter]::GetBytes($Machine).CopyTo($bytes, 0x84)
    $optionalSize = if ($Machine -eq 0x014C) { 0xE0 } else { 0xF0 }
    $magic = if ($Machine -eq 0x014C) { 0x010B } else { 0x020B }
    [BitConverter]::GetBytes([uint16]$optionalSize).CopyTo($bytes, 0x94)
    [BitConverter]::GetBytes([uint16]0x0002).CopyTo($bytes, 0x96)
    [BitConverter]::GetBytes([uint16]$magic).CopyTo($bytes, 0x98)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

function New-Artifact($Fixture, [string]$Executable, [string]$Name = 'harbor',
    [string[]]$Kind = @('bin'), [string]$Manifest = '') {
    if (-not $Manifest) { $Manifest = Join-Path $Fixture.Repo 'Cargo.toml' }
    return @{
        reason = 'compiler-artifact'
        package_id = ('path+' + ([Uri]$Fixture.Repo).AbsoluteUri + '#harbor@0.1.0')
        manifest_path = $Manifest
        target = @{ name = $Name; kind = $Kind; crate_types = $Kind; src_path = (Join-Path $Fixture.Repo 'src/main.rs') }
        profile = @{ opt_level = '0'; test = $false }
        filenames = @($Executable)
        executable = $Executable
        fresh = $true
    }
}

function New-Fixture([string]$Name, [uint16]$Machine = 0x8664,
    [string]$Version = 'cargo 1.90.0 (840b83a10 2025-07-30)') {
    $fixtureRoot = Join-Path $temporary $Name
    $repo = Join-Path $fixtureRoot 'fixture repo'
    $tools = Join-Path $fixtureRoot 'fake tools'
    New-Item -ItemType Directory -Path (Join-Path $repo 'scripts/dev'), $tools -Force | Out-Null
    Copy-Item -LiteralPath $launcher -Destination (Join-Path $repo 'scripts/dev/run_harbor.ps1')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts/dev/conpty_staging.ps1') -Destination (Join-Path $repo 'scripts/dev/conpty_staging.ps1')
    [IO.File]::WriteAllText((Join-Path $repo 'Cargo.toml'), "[package]`nname = 'harbor'`nversion = '0.1.0'`n")
    $fixture = @{
        Home = $fixtureRoot
        Repo = $repo
        Cargo = Join-Path $tools 'fake cargo.ps1'
        Config = Join-Path $tools 'cargo.json'
        Calls = Join-Path $tools 'calls.jsonl'
        Launches = Join-Path $fixtureRoot 'unexpected-launch.txt'
        Runner = Join-Path $fixtureRoot 'invoke.ps1'
        Invocation = Join-Path $fixtureRoot 'invocation.json'
        # Outside the fixture repo, with spaces: never infer target/debug or release.
        Exe = Join-Path $fixtureRoot 'shared cache/custom output/harbor.exe'
        Decoy = Join-Path $fixtureRoot 'unrelated artifacts/harbor.exe'
    }
    New-PeImage $fixture.Exe $Machine
    New-PeImage $fixture.Decoy 0x014C
    $source = Join-Path $repo 'third_party/conpty'
    foreach ($arch in @('x64', 'arm64', 'x86')) {
        $code = @{ x64 = 0x8664; arm64 = 0xAA64; x86 = 0x014C }[$arch]
        New-PeImage (Join-Path $source "$arch/OpenConsole.exe") $code
        [IO.File]::WriteAllText((Join-Path $source "$arch/conpty.dll"), "synthetic DLL: $arch")
    }
    [IO.File]::WriteAllText((Join-Path $source 'LICENSE'), 'synthetic license')
    [IO.File]::WriteAllText((Join-Path $source 'VERSION'), 'synthetic version')
    # This .ps1 deliberately returns (rather than exits) and sets LASTEXITCODE.
    # Thus failures exercise the launcher's native-exit checks, not PS exceptions.
    [IO.File]::WriteAllText($fixture.Cargo, @'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$config = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'cargo.json') -Raw | ConvertFrom-Json
$call = @{ arguments = @($args); cwd = (Get-Location).Path }
Add-Content -LiteralPath (Join-Path $PSScriptRoot 'calls.jsonl') -Value ($call | ConvertTo-Json -Compress)
if ($args.Count -eq 1 -and $args[0] -eq '--version') {
    Write-Output $config.version
    $global:LASTEXITCODE = [int]$config.versionExit
    return
}
if ($args.Count -eq 0 -or $args[0] -ne 'build') { throw 'Unexpected Cargo invocation; only version/build are permitted.' }
foreach ($record in $config.messages) { ConvertTo-Json -InputObject $record -Depth 30 -Compress }
$global:LASTEXITCODE = [int]$config.buildExit
'@)
    [IO.File]::WriteAllText($fixture.Runner, @'
param([string]$Configuration)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$config = Get-Content -LiteralPath $Configuration -Raw | ConvertFrom-Json
$global:LASTEXITCODE = 0
function Deny-Launch {
    Add-Content -LiteralPath $config.launches -Value 'unexpected application launch'
    throw 'Unexpected application launch (fixture guard).'
}
# Resolve absolute-path call-operator launches as functions, never as PE execution.
foreach ($path in $config.guardedPaths) {
    Set-Item -LiteralPath ('Function:\' + $path) -Value { Deny-Launch }
}
function Start-Process { Deny-Launch }
function Invoke-Item { Deny-Launch }
function cargo { throw 'Real Cargo is forbidden; honor -CargoPath.' }
$parameters = @{ BuildOnly = [bool]$config.buildOnly; Release = [bool]$config.release; AppArgs = [string[]]$config.appArgs }
if ($config.useDefaultCargo) {
    Set-Alias -Name cargo -Value $config.cargo
} else {
    $parameters.CargoPath = $config.cargo
}
try {
    & $config.launcher @parameters
    exit 0
} catch {
    Write-Error $_ -ErrorAction Continue
    exit 1
}
'@)
    $fixture.State = @{
        version = $Version; versionExit = 0; buildExit = 0
        messages = @(
            @{ reason = 'compiler-message'; message = @{ rendered = 'fixture diagnostic' } }
            (New-Artifact $fixture $fixture.Exe)
            # Valid-looking decoys after the correct artifact catch last-record selection.
            (New-Artifact $fixture $fixture.Decoy -Manifest (Join-Path $repo 'dependency/Cargo.toml'))
            (New-Artifact $fixture $fixture.Decoy -Kind @('lib'))
            (New-Artifact $fixture $fixture.Decoy -Name 'another-bin')
            (New-Artifact $fixture $null)
            @{ reason = 'build-finished'; success = $true }
        )
    }
    return $fixture
}

function Invoke-Fixture($Fixture, [switch]$Release, [switch]$AllowLaunch, [switch]$UseDefaultCargo) {
    Write-Json $Fixture.Config $Fixture.State
    Write-Json $Fixture.Invocation @{
        launcher = Join-Path $Fixture.Repo 'scripts/dev/run_harbor.ps1'
        cargo = $Fixture.Cargo
        launches = $Fixture.Launches
        guardedPaths = @($Fixture.Exe, $Fixture.Decoy,
            (Join-Path $Fixture.Repo 'target/debug/harbor.exe'),
            (Join-Path $Fixture.Repo 'target/release/harbor.exe'))
        buildOnly = -not $AllowLaunch; release = [bool]$Release
        useDefaultCargo = [bool]$UseDefaultCargo
        appArgs = @('--fixture-argument', 'two words')
    }
    Remove-Item -LiteralPath $Fixture.Calls -ErrorAction SilentlyContinue
    # A child contains any 'exit' in the script under test. Start from unrelated cwd.
    Push-Location $temporary
    try {
        $output = @(& $pwsh -NoLogo -NoProfile -NonInteractive -File $Fixture.Runner -Configuration $Fixture.Invocation 2>&1)
        $code = $LASTEXITCODE
    } finally { Pop-Location }
    Assert-True (-not (Test-Path -LiteralPath $Fixture.Launches)) 'Launcher attempted to run an application.'
    $calls = if (Test-Path -LiteralPath $Fixture.Calls) {
        @(Get-Content -LiteralPath $Fixture.Calls | ForEach-Object { ConvertFrom-Json -InputObject $_ })
    } else { @() }
    return @{ Code = $code; Output = ($output | Out-String); Lines = @($output | ForEach-Object { "$_" }); Calls = @($calls) }
}

function Assert-Calls($Fixture, $Result, [switch]$Release, [switch]$VersionOnly) {
    $count = if ($VersionOnly) { 1 } else { 2 }
    Assert-True ($Result.Calls.Count -eq $count) "Expected $count Cargo calls, got $($Result.Calls.Count): $($Result.Output)"
    $versionArgs = ConvertTo-Json -InputObject @($Result.Calls[0].arguments) -Compress
    Assert-True ($versionArgs -ceq '["--version"]') "Incorrect version invocation: $versionArgs"
    foreach ($call in $Result.Calls) {
        Assert-True ($call.cwd -eq $Fixture.Repo) "Cargo was not invoked from the script-relative repository root: $($call.cwd)"
    }
    if (-not $VersionOnly) {
        $expected = @('build', '--locked', '--package', 'harbor', '--bin', 'harbor', '--message-format=json-render-diagnostics')
        if ($Release) { $expected += '--release' }
        $actual = ConvertTo-Json -InputObject @($Result.Calls[1].arguments) -Compress
        Assert-True ($actual -ceq (ConvertTo-Json -InputObject $expected -Compress)) "Incorrect build arguments: $actual"
    }
}

function Get-RuntimeMap([string]$Arch) {
    return [ordered]@{
        'conpty.dll' = "$Arch/conpty.dll"
        'x64/OpenConsole.exe' = 'x64/OpenConsole.exe'
        'arm64/OpenConsole.exe' = 'arm64/OpenConsole.exe'
        'x86/OpenConsole.exe' = 'x86/OpenConsole.exe'
        'LICENSE' = 'LICENSE'
        'VERSION' = 'VERSION'
    }
}

function Assert-Runtime($Fixture, [string]$Arch) {
    $runtime = Join-Path (Split-Path -Parent $Fixture.Exe) 'conpty'
    foreach ($entry in (Get-RuntimeMap $Arch).GetEnumerator()) {
        $target = Join-Path $runtime $entry.Key
        $source = Join-Path $Fixture.Repo ('third_party/conpty/' + $entry.Value)
        Assert-True (Test-Path -LiteralPath $target -PathType Leaf) "Missing staged file: $target"
        Assert-True ((Get-FileHash -LiteralPath $target).Hash -eq (Get-FileHash -LiteralPath $source).Hash) "Incorrect runtime bytes: $target"
    }
    Assert-True (-not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $Fixture.Decoy) 'conpty'))) 'A decoy artifact was staged.'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $Fixture.Repo 'target'))) 'Launcher assumed a target directory.'
}

function Assert-Failed($Fixture, $Result, [string]$Pattern) {
    Assert-True ($Result.Code -ne 0) "Expected failure matching $Pattern, got success: $($Result.Output)"
    Assert-True ($Result.Output -match $Pattern) "Wrong failure; expected $Pattern`: $($Result.Output)"
    $runtime = Join-Path (Split-Path -Parent $Fixture.Exe) 'conpty'
    $files = @(if (Test-Path -LiteralPath $runtime) { Get-ChildItem -LiteralPath $runtime -Recurse -File })
    Assert-True ($files.Count -eq 0) 'Failure left a partial runtime beside the executable.'
}

try {
    New-Item -ItemType Directory -Path $temporary | Out-Null
    foreach ($case in @(
        @{ Arch = 'x64'; Machine = 0x8664; Version = 'cargo 1.90.0 (840b83a10 2025-07-30)'; Release = $false; Default = $false; Flavor = '(?i)standard|upstream' }
        @{ Arch = 'arm64'; Machine = 0xAA64; Version = 'cargo 1.90.0 (huacnlee/cargo shared build cache)'; Release = $true; Default = $false; Flavor = 'huacnlee/cargo' }
        @{ Arch = 'x86'; Machine = 0x014C; Version = 'cargo 1.90.0 (840b83a10 2025-07-30)'; Release = $false; Default = $true; Flavor = '(?i)standard|upstream' }
    )) {
        $fixture = New-Fixture $case.Arch $case.Machine $case.Version
        $result = Invoke-Fixture $fixture -Release:$case.Release -UseDefaultCargo:$case.Default
        Assert-True ($result.Code -eq 0) "BuildOnly failed for $($case.Arch): $($result.Output)"
        Assert-Calls $fixture $result -Release:$case.Release
        Assert-True ($result.Output -match $case.Flavor) "Missing Cargo flavor recognition: $($result.Output)"
        Assert-True ($result.Lines -ccontains $fixture.Exe) 'BuildOnly did not output the exact Cargo executable path.'
        Assert-Runtime $fixture $case.Arch
        Write-Output "PASS: $($case.Arch) PE selection, Cargo recognition/arguments, custom artifact path and complete runtime."
        if ($case.Arch -eq 'x64') { $freshFixture = $fixture }
    }

    $runtime = Join-Path (Split-Path -Parent $freshFixture.Exe) 'conpty'
    $stamp = [datetime]::new(2001, 2, 3, 4, 5, 6, [DateTimeKind]::Utc)
    foreach ($file in Get-ChildItem -LiteralPath $runtime -Recurse -File) { $file.LastWriteTimeUtc = $stamp }
    $result = Invoke-Fixture $freshFixture
    Assert-True ($result.Code -eq 0) "Fresh build failed: $($result.Output)"
    Assert-Calls $freshFixture $result
    Assert-Runtime $freshFixture 'x64'
    foreach ($file in Get-ChildItem -LiteralPath $runtime -Recurse -File) {
        Assert-True ($file.LastWriteTimeUtc -eq $stamp) "Identical file was recopied: $($file.FullName)"
    }
    Remove-Item -LiteralPath (Join-Path $runtime 'arm64/OpenConsole.exe')
    [IO.File]::WriteAllText((Join-Path $runtime 'conpty.dll'), 'corrupt staged DLL')
    $result = Invoke-Fixture $freshFixture
    Assert-True ($result.Code -eq 0) "Fresh build did not repair runtime: $($result.Output)"
    Assert-Runtime $freshFixture 'x64'
    Remove-Item -LiteralPath $runtime -Recurse -Force
    $result = Invoke-Fixture $freshFixture
    Assert-True ($result.Code -eq 0) "Fresh build did not restore deleted runtime: $($result.Output)"
    Assert-Runtime $freshFixture 'x64'
    Write-Output 'PASS: fresh artifacts skip identical hashes, repair corrupt/missing files and restore a deleted runtime.'

    $fixture = New-Fixture 'version-failure'
    $fixture.State.versionExit = 23
    $result = Invoke-Fixture $fixture -AllowLaunch
    Assert-Failed $fixture $result '(?i)cargo|version'
    Assert-Calls $fixture $result -VersionOnly
    Write-Output 'PASS: nonzero Cargo version status prevents build, staging and launch.'

    foreach ($withArtifact in @($true, $false)) {
        $fixture = New-Fixture "build-failure-$withArtifact"
        $fixture.State.buildExit = 17
        if (-not $withArtifact) { $fixture.State.messages = @(@{ reason = 'build-finished'; success = $false }) }
        $result = Invoke-Fixture $fixture -AllowLaunch
        Assert-Failed $fixture $result '(?i)build|cargo'
        Assert-Calls $fixture $result
    }
    Write-Output 'PASS: failed Cargo builds with and without an artifact never stage or launch.'

    foreach ($mode in @('no-artifact', 'null-executable', 'nonexistent-executable', 'unsupported-machine')) {
        $fixture = New-Fixture $mode
        switch ($mode) {
            'no-artifact' {
                $fixture.State.messages = @(
                    (New-Artifact $fixture $fixture.Decoy -Manifest (Join-Path $fixture.Repo 'dependency/Cargo.toml'))
                    (New-Artifact $fixture $fixture.Decoy -Kind @('lib'))
                    (New-Artifact $fixture $fixture.Decoy -Name 'another-bin')
                )
            }
            'null-executable' { $fixture.State.messages = @((New-Artifact $fixture $null)) }
            'nonexistent-executable' { Remove-Item -LiteralPath $fixture.Exe }
            'unsupported-machine' { New-PeImage $fixture.Exe 0x01C4 }
        }
        $pattern = if ($mode -eq 'unsupported-machine') { '(?i)unsupported|unknown|machine|architecture' } else { '(?i)artifact|executable' }
        $result = Invoke-Fixture $fixture -AllowLaunch
        Assert-Failed $fixture $result $pattern
        Assert-Calls $fixture $result
    }
    Write-Output 'PASS: absent/null/nonexistent root-bin artifacts and unsupported PE machines fail without staging or launch.'

    foreach ($relative in (Get-RuntimeMap 'x64').Values) {
        $fixture = New-Fixture ('missing-' + $relative.Replace('/', '-'))
        Remove-Item -LiteralPath (Join-Path $fixture.Repo "third_party/conpty/$relative")
        $result = Invoke-Fixture $fixture -AllowLaunch
        Assert-Failed $fixture $result '(?i)missing|source|conpty'
        Assert-Calls $fixture $result
    }
    Write-Output 'PASS: every required runtime source is preflighted; missing files leave no partial runtime or launch.'
    Write-Output 'PASS: all run_harbor regression checks (isolated fake Cargo, no real builds/GUI/network).'
} finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Recurse -Force }
}
