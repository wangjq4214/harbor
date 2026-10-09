# Run INSIDE Harbor's Windows PowerShell, not in the terminal used to launch Harbor.
# No config changes, app launches, focus changes, or preservation of previous clipboard data.
[CmdletBinding()]
param(
    [ValidateSet('allow', 'deny', 'confirm')][string]$Policy,
    [ValidateSet('basic', 'negative', 'capacity', 'lifecycle', 'all')][string]$Suite = 'basic',
    [string]$ReportPath,
    [switch]$AcceptSyntheticClipboardReplacement,
    [switch]$SelfTest
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Match the existing harness: avoid inherited pwsh module paths in Windows PowerShell.
if ($env:OS -eq 'Windows_NT' -and $PSVersionTable.PSEdition -eq 'Desktop') {
    Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')
    Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')
    if (!$SelfTest) { Import-Module (Join-Path $PSHOME 'Modules\CimCmdlets\CimCmdlets.psd1') }
}

function New-Osc52Fixture([string]$Name) {
    $text = 'OSC52 synthetic text' + "`n`t" + [char]0x03BB
    $selection = 'c'
    $terminator = [string][char]7
    switch ($Name) {
        'clear' { $text = '' }
        'empty-selection' { $selection = '' }
        'st' { $terminator = [string][char]27 + '\' }
        'split-st' { $terminator = [string][char]27 + '\' }
        'max' { $text = 'M' * 4194304 }
        'decoded-plus-one' { $text = 'D' * 4194305 }
        'first' { $text = 'OSC52 synthetic first request' }
        'second' { $text = 'OSC52 synthetic second request' }
    }
    $data = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text))
    switch ($Name) {
        'unpadded' { $text = 'M'; $data = 'TQ' }
        'unsupported' { $selection = 'p' }
        'query' { $data = '?' }
        'malformed' { $data = '!!!=' }
        'nul' { $data = 'QQBC' }
        'non-utf8' { $data = '//4=' }
        'encoded-plus-one' { $data = 'A' * 5592409 }
        'cancel' { $terminator = [string][char]24 }
        'incomplete' { $terminator = '' }
    }
    $wire = [string][char]27 + ']52;' + $selection + ';' + $data + $terminator
    $parts = @($wire)
    if ($Name -eq 'split-st') { $parts = @($wire.Substring(0, $wire.Length - 1), '\') }
    # Payload-bearing objects are private fixtures: never print, serialize or throw them.
    return [pscustomobject]@{ Text = $text; DataLength = $data.Length; Parts = $parts }
}

function Send-Osc52Fixture($Fixture, [IO.TextWriter]$Writer = [Console]::Out) {
    foreach ($part in $Fixture.Parts) {
        for ($offset = 0; $offset -lt $part.Length; $offset += 16384) {
            $Writer.Write($part.Substring($offset, [Math]::Min(16384, $part.Length - $offset)))
            $Writer.Flush()
        }
        if ($Fixture.Parts.Count -gt 1) { Start-Sleep -Milliseconds 50 }
    }
}

function Get-Outcome($Snapshot, [string]$Visual) {
    if ($Snapshot.Status -ne 'OK') { return 'BLOCKED' }
    if ($Visual -eq 's') { return 'NOT RUN' }
    if ($Visual -eq 'y' -and $Snapshot.Equal) { return 'PASS' }
    return 'FAIL'
}

function Get-CaseNames([string]$Mode, [string]$Group) {
    $basic = @('write', 'clear', 'empty-selection', 'unpadded', 'st', 'split-st')
    if ($Mode -eq 'confirm') { $basic = @('approve', 'deny', 'clear-approve', 'clear-deny', 'first-pending-flood') }
    $negative = @('malformed', 'unsupported', 'query', 'nul', 'non-utf8', 'cancel', 'incomplete')
    $capacity = @('max', 'decoded-plus-one', 'encoded-plus-one')
    $lifecycle = @('background', 'minimized', 'inactive-tab')
    if ($Mode -eq 'confirm') { $lifecycle += @('external-cancel', 'rapid-away-return') }
    switch ($Group) {
        'basic' { return $basic }
        'negative' { return $negative }
        'capacity' { return $capacity }
        'lifecycle' { return $lifecycle }
        'all' { return $basic + $negative + $capacity + $lifecycle }
    }
}
function Save-Osc52Report([string]$Path, $Report) {
    $directory = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Path))
    $stream = $null
    $temporaryCreated = $false
    $temporary = Join-Path $directory ('.osc52-manual-' + [Guid]::NewGuid().ToString('N') + '.tmp')
    try {
        $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $temporaryCreated = $true
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Report | ConvertTo-Json -Depth 6))
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Dispose(); $stream = $null
        # Same-directory rename is atomic; .NET Framework Move refuses existing targets.
        [IO.File]::Move($temporary, $Path)
        $temporaryCreated = $false
        return $true
    } catch { return $false } finally {
        if ($null -ne $stream) { try { $stream.Dispose() } catch {} }
        if ($temporaryCreated) { try { [IO.File]::Delete($temporary) } catch {} }
    }
}


# Native reads are owner-qualified while holding the clipboard lock. Unknown owners
# are BLOCKED, never fetched. Only equality and UTF-16 length leave this helper.
$nativeSource = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public sealed class Osc52ManualClipboard : IDisposable {
    private readonly NativeWindow owner = new NativeWindow();
    public Osc52ManualClipboard() {
        var cp = new CreateParams(); cp.Parent = new IntPtr(-3);
        owner.CreateHandle(cp);
    }
    public void Dispose() { owner.DestroyHandle(); }
    public delegate bool EnumProc(IntPtr hwnd, IntPtr arg);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr arg);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool OpenClipboard(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool CloseClipboard();
    [DllImport("user32.dll")] static extern bool EmptyClipboard();
    [DllImport("user32.dll")] static extern IntPtr SetClipboardData(uint format, IntPtr memory);
    [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
    [DllImport("user32.dll")] static extern IntPtr GetClipboardOwner();
    [DllImport("user32.dll")] static extern bool IsClipboardFormatAvailable(uint format);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalAlloc(uint flags, UIntPtr bytes);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr memory);
    [DllImport("kernel32.dll")] static extern UIntPtr GlobalSize(IntPtr memory);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalFree(IntPtr memory);
    public static uint WindowPid(IntPtr hwnd) {
        uint pid; GetWindowThreadProcessId(hwnd, out pid); return pid;
    }
    public static bool AllowedOwner(uint ownerPid, uint controller, uint harbor) {
        return ownerPid != 0 && (ownerPid == controller || ownerPid == harbor);
    }
    public static IntPtr[] Windows(uint pid) {
        var list = new List<IntPtr>();
        EnumWindows((h, a) => { if (WindowPid(h) == pid && IsWindowVisible(h)) list.Add(h); return true; }, IntPtr.Zero);
        return list.ToArray();
    }
    public bool Seed(string text) {
        IntPtr memory = GlobalAlloc(2, new UIntPtr((uint)((text.Length + 1) * 2)));
        if (memory == IntPtr.Zero) return false;
        try {
            IntPtr ptr = GlobalLock(memory); if (ptr == IntPtr.Zero) return false;
            try {
                Marshal.Copy(text.ToCharArray(), 0, ptr, text.Length);
                Marshal.WriteInt16(ptr, text.Length * 2, 0);
            } finally { GlobalUnlock(memory); }
            if (!OpenClipboard(owner.Handle)) return false;
            try {
                if (!EmptyClipboard() || SetClipboardData(13, memory) == IntPtr.Zero) return false;
                memory = IntPtr.Zero; return true; // OS owns the allocation now.
            } finally { CloseClipboard(); }
        } finally { if (memory != IntPtr.Zero) GlobalFree(memory); }
    }
    public sealed class Snapshot {
        public string Status = "BLOCKED";
        public bool Equal;
        public int Length = -1;
    }
    public static Snapshot Compare(string expected, uint controller, uint harbor) {
        var result = new Snapshot();
        if (!OpenClipboard(IntPtr.Zero)) return result;
        try {
            if (!AllowedOwner(WindowPid(GetClipboardOwner()), controller, harbor)) return result;
            if (!IsClipboardFormatAvailable(13)) {
                result.Status = "OK"; result.Length = 0; result.Equal = expected.Length == 0; return result;
            }
            IntPtr memory = GetClipboardData(13);
            // Bound reading even when a controlled writer accidentally supplies a huge value.
            ulong size = GlobalSize(memory).ToUInt64();
            if (memory == IntPtr.Zero || size < 2 || size > 16777216 || size % 2 != 0) return result;
            IntPtr ptr = GlobalLock(memory); if (ptr == IntPtr.Zero) return result;
            try {
                int count = (int)(size / 2);
                char[] chars = new char[count]; Marshal.Copy(ptr, chars, 0, count);
                int end = Array.IndexOf(chars, '\0'); if (end < 0) return result;
                string text = new string(chars, 0, end);
                result.Status = "OK"; result.Length = text.Length;
                result.Equal = String.Equals(text, expected, StringComparison.Ordinal);
                return result;
            } finally { GlobalUnlock(memory); }
        } finally { CloseClipboard(); }
    }
}
'@

if ($SelfTest) {
    # No native resources constructed, profile lookup, process discovery, or clipboard I/O.
    $checks = 0
    foreach ($name in @('write', 'clear', 'empty-selection', 'unpadded', 'st', 'split-st', 'max', 'decoded-plus-one')) {
        $fixture = New-Osc52Fixture $name
        $writer = New-Object IO.StringWriter
        try {
            Send-Osc52Fixture $fixture $writer
            $wire = $writer.ToString()
            $endLength = 1
            if ($name -in @('st', 'split-st')) { $endLength = 2 }
            $prefix = [string][char]27 + ']52;c;'
            if ($name -eq 'empty-selection') { $prefix = [string][char]27 + ']52;;' }
            if (!$wire.StartsWith($prefix, [StringComparison]::Ordinal)) { throw 'self-test framing failed' }
            $data = $wire.Substring($prefix.Length, $wire.Length - $prefix.Length - $endLength)
            if ($name -eq 'unpadded') { $data += '==' }
            $decoded = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($data))
            if ($decoded -cne $fixture.Text) { throw 'self-test fixture roundtrip failed' }
            if ($name -in @('max', 'decoded-plus-one') -and $fixture.DataLength -ne 5592408) { throw 'self-test encoded boundary failed' }
            $checks++
        } finally { $writer.Dispose() }
    }
    $overflow = New-Osc52Fixture 'encoded-plus-one'
    if ($overflow.DataLength -ne 5592409) { throw 'self-test encoded overflow failed' }
    $checks++
    foreach ($name in @('malformed', 'unsupported', 'query', 'nul', 'non-utf8', 'cancel', 'incomplete')) {
        $fixture = New-Osc52Fixture $name
        $writer = New-Object IO.StringWriter
        try {
            Send-Osc52Fixture $fixture $writer
            $wire = $writer.ToString()
            $suffix = [string][char]7
            if ($name -eq 'cancel') { $suffix = [string][char]24 }
            if ($name -eq 'incomplete') {
                if ($wire.EndsWith([string][char]7) -or $wire.EndsWith([string][char]24)) { throw 'self-test incomplete framing failed' }
            } elseif (!$wire.EndsWith($suffix, [StringComparison]::Ordinal)) { throw 'self-test rejection framing failed' }
            if ($name -eq 'unsupported' -and !$wire.StartsWith(([string][char]27 + ']52;p;'))) { throw 'self-test selection failed' }
            if ($name -eq 'nul' -and !$wire.Contains('QQBC')) { throw 'self-test NUL fixture failed' }
            if ($name -eq 'non-utf8' -and !$wire.Contains('//4=')) { throw 'self-test UTF8 fixture failed' }
            if ($name -eq 'query' -and !$wire.Contains(';?')) { throw 'self-test query failed' }
            if ($name -eq 'malformed' -and !$wire.Contains('!!!=')) { throw 'self-test malformed fixture failed' }
            $checks++
        } finally { $writer.Dispose() }
    }
    foreach ($mode in @('allow', 'deny', 'confirm')) {
        $all = @(Get-CaseNames $mode 'all')
        $groups = @(Get-CaseNames $mode 'basic') + @(Get-CaseNames $mode 'negative') + @(Get-CaseNames $mode 'capacity') + @(Get-CaseNames $mode 'lifecycle')
        if ($all.Count -ne $groups.Count -or @($all | Select-Object -Unique).Count -ne $all.Count) { throw 'self-test suite selection failed' }
        if ($mode -eq 'confirm' -and 'first-pending-flood' -notin $all) { throw 'self-test confirm coverage failed' }
        $checks++
    }
    foreach ($state in @(
        @{Status='OK'; Equal=$true; Visual='y'; Outcome='PASS'},
        @{Status='OK'; Equal=$false; Visual='y'; Outcome='FAIL'},
        @{Status='OK'; Equal=$true; Visual='n'; Outcome='FAIL'},
        @{Status='OK'; Equal=$true; Visual='s'; Outcome='NOT RUN'},
        @{Status='BLOCKED'; Equal=$true; Visual='y'; Outcome='BLOCKED'}
    )) {
        if ((Get-Outcome $state $state.Visual) -ne $state.Outcome) { throw 'self-test outcome failed' }
        $checks++
    }
    $testDirectory = Join-Path ([IO.Path]::GetTempPath()) ('osc52-selftest-' + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($testDirectory) > $null
    try {
        $path = Join-Path $testDirectory 'report.json'
        $fakeReport = [pscustomobject]@{ scope = 'self-test-only'; results = @('PASS') }
        if (!(Save-Osc52Report $path $fakeReport)) { throw 'self-test report creation failed' }
        $original = [IO.File]::ReadAllText($path)
        if (($original | ConvertFrom-Json).scope -ne 'self-test-only') { throw 'self-test report JSON failed' }
        $checks++
        if (Save-Osc52Report $path ([pscustomobject]@{ scope = 'must-not-replace' })) { throw 'self-test report collision failed' }
        if ([IO.File]::ReadAllText($path) -cne $original) { throw 'self-test prior evidence overwritten' }
        $checks++
        $blocked = Join-Path $testDirectory 'directory-not-file'
        [IO.Directory]::CreateDirectory($blocked) > $null
        if (Save-Osc52Report $blocked $fakeReport) { throw 'self-test report failure classification failed' }
        if ([IO.Directory]::GetFiles($testDirectory, '*.tmp').Length -ne 0) { throw 'self-test owned temporary report cleanup failed' }
        $checks++
    } finally { [IO.Directory]::Delete($testDirectory, $true) }
    if ($env:OS -eq 'Windows_NT') {
        Add-Type -TypeDefinition $nativeSource -ReferencedAssemblies 'System.Windows.Forms'
        if ([Osc52ManualClipboard]::AllowedOwner(0, 7, 9) -or
            [Osc52ManualClipboard]::AllowedOwner(8, 7, 9) -or
            ![Osc52ManualClipboard]::AllowedOwner(7, 7, 9) -or
            ![Osc52ManualClipboard]::AllowedOwner(9, 7, 9)) { throw 'self-test owner guard failed' }
        $checks++
    }
    Write-Output "SelfTest PASS: $checks checks; no Harbor launch or clipboard access."
    exit 0
}

if ($env:OS -ne 'Windows_NT' -or $PSVersionTable.PSEdition -ne 'Desktop' -or
    [Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') {
    throw 'Use Windows PowerShell: powershell.exe -NoProfile -STA -File scripts/test_osc52_manual.ps1 ...'
}
if (!$Policy) { throw 'Specify -Policy allow, deny, or confirm matching Harbor startup settings.' }
if (!$AcceptSyntheticClipboardReplacement) {
    throw 'Use -AcceptSyntheticClipboardReplacement. This overwrites the clipboard and does not preserve its previous contents.'
}
# Prove this shell is a descendant of Harbor before emitting terminal protocols.
$harbor = $null
$ancestor = $PID
for ($depth = 0; $depth -lt 24 -and $ancestor -gt 0; $depth++) {
    $process = Get-CimInstance Win32_Process -Filter "ProcessId=$ancestor"
    if (!$process) { break }
    if ($process.Name -ieq 'harbor.exe') { $harbor = Get-Process -Id $ancestor; break }
    if ($process.ParentProcessId -eq $ancestor) { break }
    $ancestor = [int]$process.ParentProcessId
}
if (!$harbor) { throw 'Run this script inside a Harbor tab. No clipboard was accessed and no OSC bytes were emitted.' }
Add-Type -TypeDefinition $nativeSource -ReferencedAssemblies 'System.Windows.Forms'
$windows = @([Osc52ManualClipboard]::Windows([uint32]$harbor.Id))
if ($windows.Count -ne 1) { throw 'Close existing confirmation windows first; exactly one visible Harbor window is required.' }
$mainWindow = $windows[0]
if ([Osc52ManualClipboard]::GetForegroundWindow() -ne $mainWindow -or [Osc52ManualClipboard]::IsIconic($mainWindow)) {
    throw 'Start with the original Harbor tab in the foreground and the main window restored.'
}
$rows = New-Object 'System.Collections.Generic.List[object]'
$baseline = 'OSC52 synthetic baseline'
$seed = $null
$exitCode = 2
if (!$ReportPath) { $ReportPath = Join-Path $PSScriptRoot "..\target\osc52-manual-$Policy-$Suite.json" }
$ReportPath = [IO.Path]::GetFullPath($ReportPath)
if (Test-Path $ReportPath) { throw 'Report already exists; choose a new -ReportPath rather than overwriting prior evidence.' }
$reportDirectory = [IO.Path]::GetDirectoryName($ReportPath)
[IO.Directory]::CreateDirectory($reportDirectory) > $null
$binaryHash = (Get-FileHash -LiteralPath $harbor.Path -Algorithm SHA256).Hash

function Record-Case([string]$Name, [string]$Outcome, $Snapshot, [string]$Reason, [string]$Visual = '') {
    $length = $null; $equality = $null
    if ($null -ne $Snapshot -and $Snapshot.Status -eq 'OK') { $length = $Snapshot.Length; $equality = $Snapshot.Equal }
    $rows.Add([pscustomobject]@{
        case = $Name; outcome = $Outcome; observed_utf16_length = $length; equality = $equality; reason = $Reason
        human_observation = $(switch ($Visual) { 'y' { 'verified' }; 'n' { 'failed' }; 's' { 'not-verified' }; default { 'unavailable' } })
    })
    Write-Host "$Name : $Outcome"
}
function Ask-Visual([string]$Instructions) {
    Write-Host $Instructions
    do { $reply = (Read-Host 'Expected UI/focus behavior observed? y=yes, n=no, s=not verified').Trim().ToLowerInvariant() }
    while ($reply -notin @('y', 'n', 's'))
    return $reply
}

Write-Host "Manual Harbor OSC52 test: declared startup policy=$Policy, suite=$Suite"
Write-Host 'The script does NOT verify/change your config. Restart Harbor after each policy change.'
Write-Host 'It will replace the clipboard with synthetic fixtures; do not copy other contents during testing.'
Write-Host 'Operate confirmation buttons yourself. Do not switch apps except when the case requests it.'
if ((Read-Host 'Confirm correct startup policy and consent: type REPLACE').Trim() -cne 'REPLACE') {
    throw 'Cancelled before clipboard access or protocol emission.'
}
try {
    $seed = New-Object Osc52ManualClipboard
    foreach ($name in (Get-CaseNames $Policy $Suite)) {
        $snapshot = $null
        if ([Osc52ManualClipboard]::GetForegroundWindow() -ne $mainWindow -or
            [Osc52ManualClipboard]::IsIconic($mainWindow) -or
            @([Osc52ManualClipboard]::Windows([uint32]$harbor.Id)).Count -ne 1) {
            Record-Case $name 'BLOCKED' $null 'original-main-window-not-ready'; break
        }
        Write-Host "`nCase: $name"
        if (!$seed.Seed($baseline)) { Record-Case $name 'BLOCKED' $null 'synthetic-seed-failed'; break }
        $fixtureName = $name
        if ($name -in @('approve', 'deny', 'background', 'minimized', 'inactive-tab', 'external-cancel', 'rapid-away-return')) { $fixtureName = 'write' }
        if ($name -in @('clear-approve', 'clear-deny')) { $fixtureName = 'clear' }
        if ($name -eq 'first-pending-flood') { $fixtureName = 'first' }
        $fixture = New-Osc52Fixture $fixtureName
        $rejected = $name -in @('malformed', 'unsupported', 'query', 'nul', 'non-utf8', 'cancel', 'incomplete',
            'decoded-plus-one', 'encoded-plus-one', 'deny', 'clear-deny', 'background', 'minimized', 'inactive-tab',
            'external-cancel', 'rapid-away-return')
        $expected = $baseline
        if ($Policy -ne 'deny' -and !$rejected) { $expected = $fixture.Text }
        if ($name -in @('background', 'minimized', 'inactive-tab')) {
            Write-Host 'Emission in 5 seconds. Stay in the requested state for at least 3 seconds AFTER emission.'
            if ($name -eq 'background') { Write-Host 'Switch to another application now.' }
            if ($name -eq 'minimized') { Write-Host 'Minimize Harbor now.' }
            if ($name -eq 'inactive-tab') { Write-Host 'Activate another tab now; do not close this source tab.' }
            Start-Sleep -Seconds 5
            $foreground = [Osc52ManualClipboard]::GetForegroundWindow()
            if (($name -eq 'background' -and ($foreground -eq [IntPtr]::Zero -or [Osc52ManualClipboard]::WindowPid($foreground) -eq $harbor.Id)) -or
                ($name -eq 'minimized' -and ![Osc52ManualClipboard]::IsIconic($mainWindow))) {
                Record-Case $name 'BLOCKED' $null 'requested-emission-state-not-established'; break
            }
        }
        $instructions = 'No confirmation or raw protocol bytes should appear.'
        if ($Policy -eq 'confirm' -and !$rejected) {
            $instructions = 'Check independent dialog source/decoded size/bounded preview; click Allow this request, then answer in the original tab.'
        }
        if ($Policy -eq 'confirm' -and $name -in @('deny', 'clear-deny')) {
            $instructions = 'Check the independent confirmation appears; click Deny this request, then answer in the original tab.'
        }
        if ($name -eq 'first-pending-flood') {
            $instructions = 'DO NOT APPROVE until 4 seconds after the first dialog. After the flood: exactly one dialog, still FIRST preview. Then allow once.'
        }
        if ($name -in @('external-cancel', 'rapid-away-return')) {
            $instructions = 'Do NOT approve. Verify dialog appears, switch to another app, then return (quickly for rapid-away-return). Old dialog must cancel without writing.'
        }
        if ($name -in @('background', 'minimized', 'inactive-tab')) {
            $instructions = 'After at least 3 seconds in the requested state AFTER emission, restore the original tab/window. No confirmation should appear. Attest the transition occurred.'
        }
        if ($fixtureName -in @('write', 'clear', 'empty-selection', 'unpadded', 'st', 'split-st', 'max', 'first')) {
            Write-Host ('Fixture decoded bytes: ' + [Text.Encoding]::UTF8.GetByteCount($fixture.Text))
        }
        # Give instructions BEFORE the native dialog can take focus.
        Write-Host $instructions
        Send-Osc52Fixture $fixture
        if ($name -eq 'first-pending-flood') {
            Write-Host 'Do NOT approve yet. Wait for the second/flood emissions (2 seconds).'
            Start-Sleep -Seconds 2
            $second = New-Osc52Fixture 'second'
            for ($i = 0; $i -lt 16; $i++) { Send-Osc52Fixture $second }
        }
        Start-Sleep -Seconds 2
        # Cancel an intentionally incomplete protocol before printing any further text.
        if ($name -eq 'incomplete') { [Console]::Out.Write([string][char]24); [Console]::Out.Flush() }
        $visual = Ask-Visual $instructions
        if ([Osc52ManualClipboard]::GetForegroundWindow() -ne $mainWindow -or
            @([Osc52ManualClipboard]::Windows([uint32]$harbor.Id)).Count -ne 1) {
            Record-Case $name 'BLOCKED' $null 'decision-or-main-window-not-resolved'; break
        }
        # Poll equality for asynchronous valid writes, not just terminal emission acknowledgement.
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        do {
            $snapshot = [Osc52ManualClipboard]::Compare($expected, [uint32]$PID, [uint32]$harbor.Id)
            if ($snapshot.Status -eq 'OK' -and $snapshot.Equal) { break }
            Start-Sleep -Milliseconds 50
        } while ([DateTime]::UtcNow -lt $deadline)
        $reason = 'fixture-comparison-and-human-observation'
        if ($snapshot.Status -ne 'OK') { $reason = 'clipboard-busy-uncontrolled-or-unreadable' }
        Record-Case $name (Get-Outcome $snapshot $visual) $snapshot $reason $visual
        if ($snapshot.Status -ne 'OK') { break }
    }
    $selected = @(Get-CaseNames $Policy $Suite)
    $recorded = @($rows | ForEach-Object { $_.case })
    foreach ($missing in $selected) {
        if ($missing -notin $recorded) { Record-Case $missing 'NOT RUN' $null 'earlier-case-blocked' }
    }
    if (@($rows | Where-Object { $_.outcome -eq 'FAIL' }).Count -gt 0) { $exitCode = 1 }
    elseif (@($rows | Where-Object { $_.outcome -ne 'PASS' }).Count -eq 0) { $exitCode = 0 }
} catch {
    try { [Console]::Out.Write([string][char]24); [Console]::Out.Flush() } catch {}
    # Never serialize exception messages: backend exceptions can contain payloads.
    Record-Case 'runner' 'BLOCKED' $null 'runner-error-content-redacted'
    $exitCode = 2
} finally {
    # Recover an interrupted partial control string before printing results/prompts.
    try { [Console]::Out.Write([string][char]24); [Console]::Out.Flush() } catch {}
    if ($null -ne $seed) {
        try { $seed.Dispose() } catch {
            Record-Case 'cleanup' 'BLOCKED' $null 'native-owner-cleanup-failed'
            $exitCode = 2
        }
    }
    # Include every selected case even if an exception stopped the loop.
    $recorded = @($rows | ForEach-Object { $_.case })
    foreach ($missing in (Get-CaseNames $Policy $Suite)) {
        if ($missing -notin $recorded) { Record-Case $missing 'NOT RUN' $null 'runner-stopped-before-case' }
    }
    $report = [pscustomobject]@{
        scope = 'manual-assisted-selected-cases-not-full-acceptance'; utc = [DateTime]::UtcNow.ToString('o')
        evidence_kind = 'bounded-clipboard-equality-plus-human-attestation'
        post_emission_dwell_seconds = 2; equality_timeout_seconds = 5
        fully_automated = $false
        declared_policy = $Policy; suite = $Suite; policy_verified_from_config = $false
        windows_version = [Environment]::OSVersion.Version.ToString(); powershell_version = $PSVersionTable.PSVersion.ToString()
        harbor_binary_sha256 = $binaryHash
        results = @($rows.ToArray())
        exclusions = @('emission-time-tab-identity-is-human-observed', 'source-close-late-outcome', 'query-reply-disclosure',
            'process-wide-memory-and-responsiveness', 'native-backend-failure-injection', 'ordinary-copy-paste-regressions',
            'remote-editor-WSL-SSH-tmux-compatibility')
    }
    $reportWritten = Save-Osc52Report $ReportPath $report
    if (!$reportWritten) {
        Write-Host 'Report could not be created; no existing file was overwritten.'
        $exitCode = 2
    }
    if ($reportWritten) { Write-Host "Report: $ReportPath" }
    Write-Host 'No config was modified. Clipboard now contains a synthetic fixture; original contents were not saved or restored.'
}
exit $exitCode
