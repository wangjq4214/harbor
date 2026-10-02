# Controlled Windows PowerShell -> Harbor ConPTY OSC 52 acceptance probe.
# Never read/save the pre-existing clipboard; running this intentionally replaces it.
[CmdletBinding()]
param(
    [string]$Binary,
    [string]$ReportPath,
    [switch]$AcceptSyntheticClipboardReplacement,
    # Only assert this after verifying a production startup path explicitly honors USERPROFILE.
    # dirs 6 on Windows DOES NOT. This switch is not a workaround for that implementation.
    [switch]$UserProfileIsolationVerified,
    [string]$Emitter,
    [string]$ProfileProbe
)
$ErrorActionPreference = 'Stop'
# A parent pwsh session may export a PSModulePath incompatible with Windows PowerShell.
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Osc52Native {
    public delegate bool EnumProc(IntPtr h, IntPtr arg);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr arg);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetClipboardOwner();
    [DllImport("user32.dll")] public static extern bool IsClipboardFormatAvailable(uint format);
    [DllImport("user32.dll")] static extern bool OpenClipboard(IntPtr h);
    [DllImport("user32.dll")] static extern bool CloseClipboard();
    [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr h);
    public static string ControlledText(uint controller, uint application) {
        if (!OpenClipboard(IntPtr.Zero)) throw new InvalidOperationException("clipboard busy");
        try {
            if (!IsClipboardFormatAvailable(13)) return "";
            uint pid; GetWindowThreadProcessId(GetClipboardOwner(),out pid);
            if (pid != controller && pid != application)
                throw new InvalidOperationException("uncontrolled clipboard owner; data not read");
            IntPtr handle = GetClipboardData(13);
            if (handle == IntPtr.Zero) throw new InvalidOperationException("clipboard data unavailable");
            IntPtr ptr = GlobalLock(handle);
            if (ptr == IntPtr.Zero) throw new InvalidOperationException("clipboard lock failed");
            try { return Marshal.PtrToStringUni(ptr); } finally { GlobalUnlock(handle); }
        } finally { CloseClipboard(); }
    }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int command);
    [DllImport("shell32.dll")] static extern int SHGetKnownFolderPath(ref Guid id, uint flags, IntPtr token, out IntPtr path);
    public static string Profile() {
        Guid id = new Guid("5E6C858F-0E22-4760-9AFE-EA3317B67173");
        IntPtr p; int hr = SHGetKnownFolderPath(ref id, 0, IntPtr.Zero, out p);
        if (hr != 0) throw new InvalidOperationException("known-folder lookup failed");
        try { return Marshal.PtrToStringUni(p); } finally { Marshal.FreeCoTaskMem(p); }
    }
    public static IntPtr[] Windows(int pid) {
        var result = new List<IntPtr>();
        EnumWindows((h,a) => { uint p; GetWindowThreadProcessId(h,out p);
            if (p == pid && IsWindowVisible(h)) result.Add(h); return true; }, IntPtr.Zero);
        return result.ToArray();
    }
    [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
        public ushort vk, scan; public uint flags, time; public UIntPtr extra;
    }
    [StructLayout(LayoutKind.Explicit, Size=32)] public struct INPUTUNION {
        [FieldOffset(0)] public KEYBDINPUT keyboard;
    }
    [StructLayout(LayoutKind.Sequential)] public struct INPUT {
        public uint type; public INPUTUNION data;
    }
    [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint count, INPUT[] input, int size);
    public static void Keys(ushort key, bool ctrl) {
        var items = new List<INPUT>();
        if (ctrl) items.Add(Key(0x11,0)); items.Add(Key(key,0)); items.Add(Key(key,2));
        if (ctrl) items.Add(Key(0x11,2));
        var array = items.ToArray();
        if (SendInput((uint)array.Length,array,Marshal.SizeOf(typeof(INPUT))) != array.Length)
            throw new InvalidOperationException("SendInput failed");
    }
    static INPUT Key(ushort key,uint flags) {
        return new INPUT { type=1, data=new INPUTUNION { keyboard=new KEYBDINPUT { vk=key,flags=flags } } };
    }
}
'@
if ($ProfileProbe) {
    # Same known-folder API used by dirs 6; output is only the equality bit.
    [Console]::Out.WriteLine(([Osc52Native]::Profile() -eq $env:USERPROFILE).ToString())
    exit 0
}
function Fixture([string]$Kind) {
    switch ($Kind) {
        'seed' { return 'OSC52 synthetic baseline' }
        'clear' { return '' }
        'max' { return ('M' * 4194304) }
        'decoded-plus-one' { return ('D' * 4194305) }
        default { return ('OSC52 synthetic ' + $Kind + "`n`t" + [char]0x03BB) }
    }
}
if ($Emitter) {
    # No interactive command echo. All wire bytes originate in this private child.
    # Control JSON contains fixture kinds only, never clipboard text or base64.
    $ready = Join-Path $Emitter ("ready-$PID")
    [IO.File]::WriteAllText($ready, 'ready')
    $control = Join-Path $Emitter ("control-$PID.json")
    while ($true) {
        if (!(Test-Path $control)) { Start-Sleep -Milliseconds 25; continue }
        $cmd = [IO.File]::ReadAllText($control) | ConvertFrom-Json
        Remove-Item $control
        if ($cmd.Kind -eq 'exit') { exit 0 }
        $kind = [string]$cmd.Kind
        $data = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Fixture $kind)))
        $selection = 'c'; $end = [string][char]7
        switch ($kind) {
            'empty-selection' { $selection = '' }
            'unsupported' { $selection = 'p' }
            'query' { $data = '?' }
            'malformed' { $data = '!!!=' }
            'nul' { $data = [Convert]::ToBase64String([byte[]](65,0,66)) }
            'non-utf8' { $data = [Convert]::ToBase64String([byte[]](255,254)) }
            # Encoded cap plus one, independently of decoded size validation.
            'encoded-plus-one' { $data = 'A' * 5592409 }
            'unpadded' { $data = $data.TrimEnd('=') }
            'st' { $end = ([string][char]27 + '\') }
            'split-st' { $end = ([string][char]27 + '\') }
            'cancel' { $end = [string][char]24 }
        }
        $wire = [string][char]27 + ']52;' + $selection + ';' + $data + $end
        if ($kind -eq 'incomplete') { $wire = $wire.Substring(0,$wire.Length-1) }
        if ($kind -eq 'split-st') {
            [Console]::Out.Write($wire.Substring(0,$wire.Length-1)); [Console]::Out.Flush()
            Start-Sleep -Milliseconds 50
            [Console]::Out.Write('\'); [Console]::Out.Flush()
            $wire = ''
        }
        if ($kind -eq 'flood') {
            # This command is sent only after an earlier request's dialog exists.
            $wire = $wire * 16
        }
        for ($i=0; $i -lt $wire.Length; $i+=16384) {
            [Console]::Out.Write($wire.Substring($i,[Math]::Min(16384,$wire.Length-$i)))
            [Console]::Out.Flush()
        }
        [IO.File]::WriteAllText((Join-Path $Emitter ("ack-" + $cmd.Id)), 'flushed')
    }
}
if (!$AcceptSyntheticClipboardReplacement) {
    throw 'Use -AcceptSyntheticClipboardReplacement: this replaces the clipboard and does not preserve unrelated contents.'
}
if (![Environment]::Is64BitProcess -or $PSVersionTable.PSEdition -ne 'Desktop') {
    throw 'Run 64-bit Windows PowerShell powershell.exe -NoProfile -STA -File ...'
}
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (!$Binary) { $Binary = Join-Path $root 'target\debug\harbor.exe' }
$Binary = [IO.Path]::GetFullPath($Binary)
if (!(Test-Path $Binary)) { throw 'Build harbor.exe first (cargo build --bin harbor).' }
$work = Join-Path ([IO.Path]::GetTempPath()) ('harbor-osc52-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $work
$results = New-Object 'System.Collections.Generic.List[object]'
$children = New-Object 'System.Collections.Generic.List[System.Diagnostics.Process]'
$previousFocus = [Osc52Native]::GetForegroundWindow()
$rows = @('default-write','default-clear','allow-write','allow-clear','empty-selection','unpadded','st','split-st',
    'deny-write','deny-clear','confirm-approve','confirm-clear-approve','confirm-deny','confirm-clear-deny',
    'confirm-first-pending-flood','confirmation-focus-continuity','external-focus-cancel','minimized-write',
    'minimized-clear','unfocused-write','inactive-tab-write','tab-change-cancel','source-close-stale',
    'malformed','unsupported','query','nul','non-utf8','cancel','incomplete','exact-decoded-max',
    'decoded-max-plus-one','encoded-cap-plus-one')
function Record($Name,$Outcome,$Length=$null,$Equal=$null,$Reason='') {
    $entry = [pscustomobject]@{ case=$Name; outcome=$Outcome; observed_length=$Length; equality=$Equal; reason=$Reason }
    $results.Add($entry)
    Write-Output ($entry | ConvertTo-Json -Compress)
}
function Start-PrivateProcess([string]$Exe,[string]$Arguments,[string]$Profile) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName=$Exe; $info.Arguments=$Arguments; $info.UseShellExecute=$false
    $info.WorkingDirectory=$root; $info.EnvironmentVariables['USERPROFILE']=$Profile
    $info.EnvironmentVariables['HOME']=$Profile
    $info.EnvironmentVariables['RUST_LOG']='off'
    # Drain and discard; never retain app diagnostic/terminal contents.
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    $p = New-Object Diagnostics.Process; $p.StartInfo=$info
    $null=$p.Start(); $children.Add($p)
    return $p
}
function Wait-Until([scriptblock]$Check,[int]$Seconds=10) {
    $stop=(Get-Date).AddSeconds($Seconds)
    do { if (& $Check) { return $true }; Start-Sleep -Milliseconds 50 } while ((Get-Date) -lt $stop)
    return $false
}
$report = [ordered]@{
    timestamp_utc=[DateTime]::UtcNow.ToString('o')
    revision=((& git -C $root rev-parse HEAD) -join '')
    binary_sha256=(Get-FileHash -Algorithm SHA256 $Binary).Hash
    binary_last_write_utc=(Get-Item $Binary).LastWriteTimeUtc.ToString('o')
    profile='dev'; windows=[Environment]::OSVersion.Version.ToString()
    powershell=$PSVersionTable.PSVersion.ToString()
    powershell_file_version=(Get-Item (Join-Path $PSHOME 'powershell.exe')).VersionInfo.FileVersion
    conhost_file_version=(Get-Item "$env:WINDIR\System32\conhost.exe").VersionInfo.FileVersion
    conpty_dll_file_version=if (Test-Path "$env:WINDIR\System32\conpty.dll") { (Get-Item "$env:WINDIR\System32\conpty.dll").VersionInfo.FileVersion } else { 'absent; inbox API path' }
    intended_transport='direct synthetic Windows PowerShell Console.Out -> Harbor ConPTY -> parser -> host -> OS clipboard'
    application_transport_exercised=$false
    rustc=((& rustc --version) -join '')
    results=$results
}
try {
    $probe = Start-PrivateProcess (Join-Path $PSHOME 'powershell.exe') ('-NoProfile -File "' + $PSCommandPath + '" -ProfileProbe "' + $work + '"') $work
    $probeOutput=$probe.StandardOutput.ReadToEnd().Trim()
    $null=$probe.StandardError.ReadToEnd(); $probe.WaitForExit()
    $report['dirs_known_folder_matches_child_userprofile']=($probeOutput -eq 'True')
    if ($probeOutput -ne 'True' -and !$UserProfileIsolationVerified) {
        Record 'startup-config-isolation' 'BLOCKED' $null $false 'dirs6-known-folder-ignores-USERPROFILE; real config untouched; application not launched'
        foreach ($name in $rows) { Record $name 'BLOCKED' $null $null 'safe private startup configuration unavailable' }
    } else {
        Add-Type -AssemblyName System.Windows.Forms
        $external = New-Object Windows.Forms.Form
        $external.Text='OSC52 synthetic external-focus target'; $external.Width=350; $external.Height=120
        $external.Show(); [Windows.Forms.Application]::DoEvents()
        function Focus([IntPtr]$h) {
            $null=[Osc52Native]::ShowWindow($h,9); $null=[Osc52Native]::SetForegroundWindow($h)
            if (!(Wait-Until { [Osc52Native]::GetForegroundWindow() -eq $h } 2)) { throw 'Controlled foreground acquisition failed' }
            Start-Sleep -Milliseconds 200
        }
        function Seed { [Windows.Forms.Clipboard]::SetText((Fixture 'seed')) }
        function Observe($Name,[string]$Expected,[int]$Dwell=1) {
            Start-Sleep -Seconds $Dwell
            # Hold the OS clipboard lock while checking ownership and fetching text.
            # A foreign/unknown owner is never read. Empty text needs no data fetch.
            try { $actual=[Osc52Native]::ControlledText($PID,$s.App.Id) }
            catch {
                Record $Name 'BLOCKED' $null $null 'clipboard busy or ownership unproven; uncontrolled text not read'
                return
            }
            $same=[string]::Equals($actual,$Expected,[StringComparison]::Ordinal)
            Record $Name $(if ($same) {'PASS'} else {'FAIL'}) $actual.Length $same
            $actual=$null
        }
        function Launch([string]$Policy) {
            $dir=Join-Path $work ([Guid]::NewGuid().ToString('N'))
            $null=New-Item -ItemType Directory -Path (Join-Path $dir '.harbor')
            $ctl=Join-Path $dir 'control'; $null=New-Item -ItemType Directory -Path $ctl
            # TOML literal strings preserve Windows paths, with no manual escaping.
            $cfg="[shell]`nprogram = 'powershell.exe'`nargs = ['-NoProfile', '-File', '$PSCommandPath', '-Emitter', '$ctl']`n"
            if ($Policy -ne 'default') { $cfg+="[clipboard]`nosc52_write = '$Policy'`n" }
            [IO.File]::WriteAllText((Join-Path $dir '.harbor\config.toml'),$cfg)
            $app=Start-PrivateProcess $Binary '' $dir
            $app.BeginOutputReadLine(); $app.BeginErrorReadLine()
            if (!(Wait-Until { @(Get-ChildItem $ctl -Filter 'ready-*').Count -gt 0 } 12)) {
                throw 'Private emitter did not start: stop, do not modify real user settings'
            }
            $ready=@(Get-ChildItem $ctl -Filter 'ready-*')[0]
            $source=[int]($ready.Name.Substring(6))
            $hwnds=[Osc52Native]::Windows($app.Id)
            if ($hwnds.Count -ne 1) { throw 'Main HWND discovery ambiguous' }
            Focus $hwnds[0]
            $report['application_transport_exercised']=$true
            return [pscustomobject]@{ App=$app; Main=$hwnds[0]; Control=$ctl; Source=$source }
        }
        function Emit($Session,[string]$Kind) {
            $id=[Guid]::NewGuid().ToString('N')
            $cmd=@{Id=$id;Kind=$Kind}|ConvertTo-Json -Compress
            $tmp=Join-Path $Session.Control ('temp-'+$id)
            [IO.File]::WriteAllText($tmp,$cmd)
            Move-Item $tmp (Join-Path $Session.Control ("control-"+$Session.Source+'.json'))
            if (!(Wait-Until { Test-Path (Join-Path $Session.Control ('ack-'+$id)) } 30)) { throw 'Emitter flush timeout' }
        }
        function Dialog($Session) {
            $script:dialog=[IntPtr]::Zero
            $found=Wait-Until {
                $others=@([Osc52Native]::Windows($Session.App.Id)|Where-Object { $_ -ne $Session.Main })
                if ($others.Count -eq 1) { $script:dialog=$others[0]; return $true }; return $false
            } 5
            if (!$found) { throw 'Independent confirmation HWND not observed' }
            Focus $script:dialog
            return $script:dialog
        }
        function Decision([IntPtr]$Window,[bool]$Approve) {
            Focus $Window; [Osc52Native]::Keys($(if ($Approve) {0x59} else {0x4E}),$false)
            if (!(Wait-Until { ![Osc52Native]::IsWindow($Window) } 5)) { throw 'Confirmation did not close after y/n' }
        }
        foreach ($policy in @('default','allow','deny')) {
            $s=Launch $policy
            foreach ($kind in @('write','clear')) {
                Seed; Focus $s.Main; Emit $s $kind
                $expected=if ($policy -eq 'deny') {Fixture 'seed'} else {Fixture $kind}
                Observe "$policy-$kind" $expected
            }
            if ($policy -eq 'allow') {
                foreach ($kind in @('empty-selection','unpadded','st','split-st')) {
                    Seed; Focus $s.Main; Emit $s $kind; Observe $kind (Fixture $kind)
                }
                foreach ($kind in @('malformed','unsupported','query','nul','non-utf8','cancel')) {
                    Seed; Focus $s.Main; Emit $s $kind; Observe $kind (Fixture 'seed')
                }
                foreach ($kind in @('write','clear')) {
                    Seed; $null=[Osc52Native]::ShowWindow($s.Main,6)
                    if (![Osc52Native]::IsIconic($s.Main)) { throw 'Minimize precondition failed' }
                    Emit $s $kind; Observe "minimized-$kind" (Fixture 'seed'); Focus $s.Main
                }
                Seed; Focus $external.Handle; Emit $s 'write'; Observe 'unfocused-write' (Fixture 'seed')
                Seed; Focus $s.Main; [Osc52Native]::Keys(0x54,$true)
                if (Wait-Until { @(Get-ChildItem $s.Control -Filter 'ready-*').Count -eq 2 } 5) {
                    Emit $s 'write'; Observe 'inactive-tab-write' (Fixture 'seed')
                    [Osc52Native]::Keys(0x31,$true); Start-Sleep -Milliseconds 500
                } else { Record 'inactive-tab-write' 'BLOCKED' $null $null 'second emitter not observed' }
                foreach ($item in @(@('max','exact-decoded-max'),@('decoded-plus-one','decoded-max-plus-one'),@('encoded-plus-one','encoded-cap-plus-one'))) {
                    Seed; Focus $s.Main; Emit $s $item[0]
                    $expected=if ($item[0] -eq 'max') { Fixture 'max' } else { Fixture 'seed' }
                    Observe $item[1] $expected 5
                }
                # Incomplete framing is last in this process; no later request is consumed into it.
                Seed; Focus $s.Main; Emit $s 'incomplete'; Observe 'incomplete' (Fixture 'seed')
            }
            $s.App.Kill(); $s.App.WaitForExit()
        }
        $s=Launch 'confirm'
        foreach ($item in @(@('write',$true,'confirm-approve'),@('clear',$true,'confirm-clear-approve'),@('write',$false,'confirm-deny'),@('clear',$false,'confirm-clear-deny'))) {
            Seed; Focus $s.Main; Emit $s $item[0]; $d=Dialog $s
            # Main -> independent confirmation focus is intentionally maintained before approval.
            Start-Sleep -Milliseconds 500; Decision $d $item[1]
            $expected=if ($item[1]) {Fixture $item[0]} else {Fixture 'seed'}
            Observe $item[2] $expected
        }
        $approved=@($results|Where-Object { $_.case -eq 'confirm-approve' -and $_.outcome -eq 'PASS' }).Count -eq 1
        Record 'confirmation-focus-continuity' $(if ($approved) {'PASS'} else {'FAIL'}) $null $approved 'independent HWND foreground before approval; depends on exact confirm-approve equality' 
        Seed; Focus $s.Main; Emit $s 'write'; $d=Dialog $s; Emit $s 'flood'
        $others=@([Osc52Native]::Windows($s.App.Id)|Where-Object { $_ -ne $s.Main })
        if ($others.Count -ne 1 -or $others[0] -ne $d) { throw 'Flood changed confirmation window identity/count' }
        Decision $d $true; Observe 'confirm-first-pending-flood' (Fixture 'write')
        Seed; Focus $s.Main; Emit $s 'write'; $d=Dialog $s; Focus $external.Handle
        $cancelled=Wait-Until { ![Osc52Native]::IsWindow($d) } 5
        if ($cancelled) { Observe 'external-focus-cancel' (Fixture 'seed') } else { Record 'external-focus-cancel' 'FAIL' $null $false 'dialog remained after external focus' }
        Record 'tab-change-cancel' 'NOT RUN' $null $null 'requires separately verified active-tab transition while modal input gate is pending'
        Record 'source-close-stale' 'NOT RUN' $null $null 'requires controlled source-close and stale callback seam; process shutdown alone is not proof'
        Seed
    }
} catch {
    # Exception text can be provider-controlled. Only fixed classification is recorded.
    Record 'harness-execution' 'BLOCKED' $null $null 'controlled precondition or runtime operation failed; no exception/payload captured'
    foreach ($name in $rows) { if (!@($results|Where-Object case -eq $name).Count) { Record $name 'NOT RUN' $null $null 'runner stopped before case' } }
} finally {
    foreach ($p in $children) { try { if (!$p.HasExited) { $p.Kill(); $p.WaitForExit() }; $p.Dispose() } catch {} }
    if ($external) { $external.Close(); $external.Dispose() }
    if ([Osc52Native]::IsWindow($previousFocus)) { $null=[Osc52Native]::SetForegroundWindow($previousFocus) }
    Remove-Item -Recurse -Force $work
    if ($ReportPath) { [IO.File]::WriteAllText([IO.Path]::GetFullPath($ReportPath),($report|ConvertTo-Json -Depth 6)) }
}
if (@($results|Where-Object outcome -eq 'FAIL').Count) { exit 1 }
if (@($results|Where-Object { $_.outcome -in @('BLOCKED','NOT RUN') }).Count) { exit 2 }
exit 0
