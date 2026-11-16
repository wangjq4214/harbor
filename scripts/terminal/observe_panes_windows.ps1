#Requires -Version 7.0
<#
.SYNOPSIS
Bounded synthetic native-window pane observation, not manual desktop acceptance.
.DESCRIPTION
Uses a new portable copy, dedicated local pwsh children and HWND-targeted messages.
Never injects global keys, requests foreground activation, reads/writes the clipboard,
captures the desktop, installs integration or connects to remote targets.
Custom single-key bindings exercise command dispatch, NOT the default Ctrl chords.
Requires inherited pwsh profiles not to auto-source context integration for the
unknown-context subcase. Normal OS activation on window creation is not suppressed.
Artifacts are retained in a caller-selected NEW directory. Run serially.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Windows required.' }
$exe = (Resolve-Path -LiteralPath $Executable).Path
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$out = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $out) { throw 'OutputDirectory must not exist; no artifacts are overwritten.' }
$bundle = Join-Path (Split-Path -Parent $exe) 'conpty'
if (-not (Test-Path -LiteralPath $bundle)) { throw 'Build/stage the executable with scripts/dev/run_harbor.ps1 -BuildOnly first.' }
Add-Type -Namespace PaneObservation -Name Native -MemberDefinition @'
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, UIntPtr w, IntPtr l);
[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint);
[DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
'@
[void](New-Item -ItemType Directory -Path $out)
$portable = Join-Path $out 'portable'
$current = Join-Path $out 'current'
$initial = Join-Path $out 'initial'
foreach ($dir in @($portable, $current, $initial)) { [void](New-Item -ItemType Directory -Path $dir) }
Copy-Item -LiteralPath $exe -Destination (Join-Path $portable 'harbor.exe')
Copy-Item -LiteralPath $bundle -Destination (Join-Path $portable 'conpty') -Recurse
Set-Content -LiteralPath (Join-Path $portable 'harbor.portable') -Value ''
# Only explicit synthetic values are recorded. No runtime environment dump.
$report = @'
@{ pid = $PID; cwd = (Get-Location).Path; pwsh = $PSVersionTable.PSVersion.ToString() } |
    ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $PSScriptRoot "child-$PID.json")
'@
Set-Content -LiteralPath (Join-Path $current 'report.ps1') -Value $report
Set-Content -LiteralPath (Join-Path $current 'output.ps1') -Value @'
1..2000 | ForEach-Object { [Console]::WriteLine("T0005-NATIVE-ROW-$_") }
Set-Content -LiteralPath (Join-Path $PSScriptRoot "output-$PID.txt") -Value 'producer-complete-2000'
'@
$quote = { param($s) "'" + $s.Replace("'", "''") + "'" }
$startup = Join-Path $out 'startup.ps1'
Set-Content -LiteralPath $startup -Value @"
. $(& $quote (Join-Path $repo 'scripts/shell/harbor-project.ps1'))
Set-Location -LiteralPath $(& $quote $current)
. ./report.ps1
Set-Content -LiteralPath (Join-Path $(& $quote $current) "startup-`$PID.txt") -Value 'accepted-default-startup'
"@
$pwsh = (Get-Command pwsh -CommandType Application | Select-Object -First 1).Source
# JSON string escaping is also valid for these TOML basic strings.
Set-Content -LiteralPath (Join-Path $portable 'config.toml') -Value @"
[shell]
program = $(ConvertTo-Json -InputObject $pwsh -Compress)
args = ['-NoLogo', '-NoProfile', '-NoExit', '-File', $(ConvertTo-Json -InputObject $startup -Compress)]
directory = $(ConvertTo-Json -InputObject $initial -Compress)
[keybindings.app.split-horizontal]
bindings = [{ modifiers = [], key = 'j' }]
[keybindings.app.split-vertical]
bindings = [{ modifiers = [], key = 'k' }]
[keybindings.app.maximize-pane]
bindings = [{ modifiers = [], key = 'm' }]
[keybindings.app.next-pane]
bindings = [{ modifiers = [], key = 'n' }]
[keybindings.app.close-pane]
bindings = [{ modifiers = [], key = 'b' }]
[keybindings.app.close-active-tab]
bindings = [{ modifiers = [], key = 'v' }]
"@
$app = $null
$events = [Collections.Generic.List[object]]::new()
function Record($phase, $data) { $events.Add(@{ phase = $phase; data = $data; utc = [DateTime]::UtcNow.ToString('o') }) }
function Wait-Until([scriptblock]$condition, [string]$label) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while (-not (& $condition)) {
        if ($app.HasExited) { throw "App exited during $label (exit $($app.ExitCode))." }
        if ($timer.Elapsed.TotalSeconds -gt 25) { throw "Timed out: $label" }
        Start-Sleep -Milliseconds 100
    }
}
function Check-OwnedWindow {
    $app.Refresh()
    $script:hwnd = $app.MainWindowHandle
    [uint32]$owner = 0
    if ($app.HasExited -or $hwnd -eq [IntPtr]::Zero -or
        -not [PaneObservation.Native]::IsWindow($hwnd)) { throw 'Owned native window unavailable.' }
    [void][PaneObservation.Native]::GetWindowThreadProcessId($hwnd, [ref]$owner)
    if ($owner -ne $app.Id) { throw 'HWND owner mismatch; refusing message.' }
}
function Key([int]$vk) {
    Check-OwnedWindow
    if (-not [PaneObservation.Native]::PostMessage($hwnd, 0x100, [UIntPtr]$vk, [IntPtr]1)) { throw 'keydown failed' }
    Start-Sleep -Milliseconds 30
    if ($app.HasExited) { return }
    Check-OwnedWindow
    if (-not [PaneObservation.Native]::PostMessage($hwnd, 0x101, [UIntPtr]$vk, [IntPtr]0xC0000001L)) { throw 'keyup failed' }
    Start-Sleep -Milliseconds 30
}
function Line([string]$s) {
    foreach ($c in $s.ToCharArray()) {
        $vk = switch ($c) { '.' { 0xBE } '/' { 0xBF } default { [int][char]::ToUpperInvariant($c) } }
        Key $vk
    }
    Key 13
}
function Reports { @(Get-ChildItem -LiteralPath $current -Filter 'child-*.json') }
function Live-ReportedChildren {
    @(Reports | ForEach-Object {
        $child = Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
        Get-Process -Id $child.pid -ErrorAction SilentlyContinue
    })
}
try {
    $psi = [Diagnostics.ProcessStartInfo]::new((Join-Path $portable 'harbor.exe'))
    $psi.WorkingDirectory = $out
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $app = [Diagnostics.Process]::Start($psi)
    Wait-Until { $app.Refresh(); $app.MainWindowHandle -ne [IntPtr]::Zero -and @(Reports).Count -eq 1 } 'source window/report'
    Check-OwnedWindow
    $sourceReport = @(Reports)[0]
    $sourceChild = Get-Content -LiteralPath $sourceReport.FullName -Raw | ConvertFrom-Json
    Record 'native-window' @{ pid = $app.Id; hwnd = $hwnd.ToInt64(); initial = $initial; current = $current }
    # Source prompt must publish current context before each split.
    Start-Sleep -Seconds 2
    Key 0x4A # j: horizontal
    Start-Sleep -Seconds 3
    Line './report.ps1'
    Wait-Until { @(Reports).Count -eq 2 } 'horizontal child report'
    if (@(Get-ChildItem -LiteralPath $current -Filter 'startup-*.txt').Count -ne 1) { throw 'Horizontal split unexpectedly used default startup.' }
    Record 'horizontal-inherited' @{ child_reports = 2; default_startups = @(Get-ChildItem -LiteralPath $current -Filter 'startup-*.txt').Count }
    # The inherited shell has no prompt integration installed: unknown context
    # must use the accepted default command, which creates a startup receipt.
    Key 0x4A
    Wait-Until { @(Reports).Count -eq 3 } 'unknown-context default fallback'
    if (@(Get-ChildItem -LiteralPath $current -Filter 'startup-*.txt').Count -ne 2) { throw 'Expected exactly one additional default startup.' }
    Record 'unknown-context-default' @{ child_reports = 3; default_startups = 2 }
    Key 0x4E # n: from new leaf wrap to original, whose prompt integration remains installed
    Start-Sleep -Seconds 1
    Key 0x4B # k: vertical from original
    Start-Sleep -Seconds 3
    Line './report.ps1'
    Wait-Until { @(Reports).Count -eq 4 } 'vertical child report'
    $children = @(Reports | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json })
    if (@($children | Where-Object cwd -NE $current).Count -ne 0) { throw 'Child cwd mismatch.' }
    if (@(Get-ChildItem -LiteralPath $current -Filter 'startup-*.txt').Count -ne 2) { throw 'Vertical split unexpectedly used default startup.' }
    Record 'four-child-reports' $children
    # Focus cycles insertion order, not geometric tree order. The vertical child
    # was added last, so one Next wraps to the original source.
    Key 0x4E
    $stamp = (Get-Item -LiteralPath $sourceReport.FullName).LastWriteTimeUtc
    Line './report.ps1'
    Wait-Until { (Get-Item -LiteralPath $sourceReport.FullName).LastWriteTimeUtc -gt $stamp } 'original shell responsiveness'
    Record 'original-still-responsive' @{ pid = $sourceChild.pid; cwd = $sourceChild.cwd }
    Check-OwnedWindow
    if (-not [PaneObservation.Native]::MoveWindow($hwnd, 30, 30, 1100, 700, $true)) { throw 'owned resize failed' }
    Key 0x4D # maximize focused pane
    Line './output.ps1'
    Wait-Until { @(Get-ChildItem -LiteralPath $current -Filter 'output-*.txt').Count -eq 1 } 'bounded producer'
    Key 0x4D # restore
    Start-Sleep -Seconds 2
    $app.Refresh()
    $cpu = $app.TotalProcessorTime.TotalMilliseconds
    $handles = $app.HandleCount
    $private = $app.PrivateMemorySize64
    Start-Sleep -Seconds 5
    $app.Refresh()
    Record 'quiet-five-seconds' @{ cpu_ms = $app.TotalProcessorTime.TotalMilliseconds - $cpu; handles_before = $handles; handles_after = $app.HandleCount; private_before = $private; private_after = $app.PrivateMemorySize64 }
    # Natural exit of focused child, explicit close survivor, whole-tab/final window.
    Line 'exit'
    Start-Sleep -Seconds 2
    Wait-Until { @(Live-ReportedChildren).Count -eq 3 } 'natural shell exit'
    Record 'natural-exit' @{ live_reported_children = 3 }
    Key 0x42 # b: close one pane
    Start-Sleep -Seconds 2
    Wait-Until { @(Live-ReportedChildren).Count -eq 2 } 'explicit pane close'
    Record 'explicit-pane-close' @{ live_reported_children = 2 }
    Key 0x56 # v: close whole tab
    if (-not $app.WaitForExit(15000)) { throw 'Final tab did not exit the native app.' }
    if ($app.ExitCode -ne 0) { throw "App exit code $($app.ExitCode)" }
    Start-Sleep -Seconds 1
    if (@(Live-ReportedChildren).Count -ne 0) { throw 'Reported dedicated shell still alive after final close.' }
    Record 'completed' @{ exit_code = $app.ExitCode; producer_rows = 2000; window_messages_only = $true }
} catch {
    Record 'failed' @{ reason = $_.Exception.Message }
    throw
} finally {
    if ($null -ne $app -and -not $app.HasExited) {
        [void]$app.CloseMainWindow()
        if (-not $app.WaitForExit(5000)) { $app.Kill($true); $app.WaitForExit() }
        Record 'cleanup' @{ app_exited = $app.HasExited }
    }
    ConvertTo-Json -InputObject @($events.ToArray()) -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'observation.json')
}
