<#
.SYNOPSIS
    OSC palette probe v1: run in a dedicated, freshly built Harbor/ConPTY session.
.DESCRIPTION
    Captures application-visible replies and displays retained indexed/truecolor
    samples through baseline, set, selective reset, and full reset phases.
    Each phase pauses for -PhaseSeconds for screenshots. Query timeouts are
    BLOCKED, not PASS; the CPR control distinguishes input-path prerequisites.
    Restores the initial indexed palette and console modes on completion.
#>
[CmdletBinding()]
param(
    [string]$ReportPath,
    [int]$PhaseSeconds = 3
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class OscPaletteProbe {
    [DllImport("kernel32.dll")] public static extern IntPtr GetStdHandle(int n);
    [DllImport("kernel32.dll")] public static extern bool GetConsoleMode(IntPtr h, out uint m);
    [DllImport("kernel32.dll")] public static extern bool SetConsoleMode(IntPtr h, uint m);
    [DllImport("kernel32.dll")] static extern bool ReadFile(IntPtr h, byte[] b, uint n, out uint read, IntPtr overlapped);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] static extern IntPtr OpenThread(uint access, bool inherit, uint id);
    [DllImport("kernel32.dll")] static extern bool CancelSynchronousIo(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    public static string Exchange(string request, int timeoutMs) {
        var result = new StringBuilder();
        string failure = null;
        uint threadId = 0;
        var started = new System.Threading.ManualResetEventSlim();
        var worker = new System.Threading.Thread(() => {
            threadId = GetCurrentThreadId();
            started.Set();
            var bytes = new byte[256];
            while (ReadFile(GetStdHandle(-10), bytes, 256, out uint read, IntPtr.Zero) && read > 0) {
                result.Append(Encoding.ASCII.GetString(bytes, 0, (int)read));
                if (result.Length > 1024) { failure = "BLOCKED: unexpected input exceeded probe bound"; return; }
                string s = result.ToString();
                if (s.EndsWith("\a") || s.EndsWith("\x1b\\") ||
                    (request == "\x1b[6n" && s.EndsWith("R"))) return;
            }
        });
        worker.IsBackground = true;
        worker.Start();
        started.Wait();
        Console.Out.Write(request);
        Console.Out.Flush();
        if (!worker.Join(timeoutMs)) {
            IntPtr h = OpenThread(1, false, threadId);
            if (h != IntPtr.Zero) { CancelSynchronousIo(h); CloseHandle(h); }
            if (!worker.Join(1000)) throw new InvalidOperationException("BLOCKED: console read cancellation failed");
        }
        started.Dispose();
        if (failure != null) throw new InvalidOperationException(failure);
        return result.ToString();
    }
}
'@
$inputHandle = [OscPaletteProbe]::GetStdHandle(-10)
$outputHandle = [OscPaletteProbe]::GetStdHandle(-11)
[uint32]$inputMode = 0
[uint32]$outputMode = 0
if (-not [OscPaletteProbe]::GetConsoleMode($inputHandle, [ref]$inputMode) -or
    -not [OscPaletteProbe]::GetConsoleMode($outputHandle, [ref]$outputMode)) {
    throw 'BLOCKED: run inside a real Harbor console session, not redirected I/O.'
}
$e = [char]27
$bel = [char]7
$results = [System.Collections.Generic.List[object]]::new()
function Query([string]$name, [string]$request, [string]$expected = '') {
    $observed = [OscPaletteProbe]::Exchange($request, 1500)
    $shape = if ($request -eq "$e[6n") { $observed -match '^\x1b\[\d+;\d+R$' } else {
        $observed -match '^\x1b\]4;\d{1,3};rgb:[0-9a-f]{4}/[0-9a-f]{4}/[0-9a-f]{4}(\x07|\x1b\\)$' -and
        (($request.EndsWith("$bel") -and $observed.EndsWith("$bel")) -or ($request.EndsWith("$e\") -and $observed.EndsWith("$e\")))
    }
    $outcome = if ($observed.Length -eq 0) { 'BLOCKED' } elseif (-not $shape -or ($expected -and $observed -ne $expected)) { 'FAIL' } else { 'PASS' }
    $results.Add([pscustomobject]@{ Name = $name; Request = $request; Expected = $expected; Observed = $observed; Outcome = $outcome })
    return $observed
}
function Phase([string]$name) {
    [Console]::Out.Write("$e[1;1H$e[0mOSC palette probe v1: $name$e[K")
    [Console]::Out.Flush()
    Start-Sleep -Seconds $PhaseSeconds
}
try {
    # Raw, unechoed input with VT input and VT output; restore exact modes below.
    if (-not [OscPaletteProbe]::SetConsoleMode($inputHandle, (($inputMode -band (-bnot 6)) -bor 512)) -or
        -not [OscPaletteProbe]::SetConsoleMode($outputHandle, ($outputMode -bor 4))) {
        throw 'BLOCKED: console mode setup failed.'
    }
    [Console]::Out.Write("$e[0m$e[2J$e[H$e[?25l")
    $control = Query 'CPR input control' "$e[6n"
    if ($control -notmatch '^\x1b\[\d+;\d+R$') { throw 'BLOCKED: CPR control failed; palette replies cannot be accepted.' }
    $baseline1 = Query 'Startup index 1' "$e]4;1;?$bel"
    $baseline42 = Query 'Startup index 42' "$e]4;42;?$e\"
    if ($results[$results.Count - 1].Outcome -ne 'PASS' -or $results[$results.Count - 2].Outcome -ne 'PASS') {
        throw 'BLOCKED: valid startup palette replies are required for reset comparisons.'
    }
    [Console]::Out.Write("$e[3;1H$e[38;5;42mIndexed foreground XXX$e[0m")
    [Console]::Out.Write("$e[4;1H$e[48;5;42mIndexed background    $e[0m")
    [Console]::Out.Write("$e[5;1H$e[4;58;5;42mIndexed underline    $e[0m")
    [Console]::Out.Write("$e[6;1H$e[31mANSI alias XXX$e[0m")
    [Console]::Out.Write("$e[7;1H$e[38;2;18;52;86mTruecolor control XXX$e[0m")
    Phase 'BASELINE'
    [Console]::Out.Write("$e]4;42;#123456;1;#ff00ff$bel")
    $null = Query 'Exact ST set/query 42' "$e]4;42;?$e\" "$e]4;42;rgb:1212/3434/5656$e\"
    $null = Query 'Exact BEL ANSI query 1' "$e]4;1;?$bel" "$e]4;1;rgb:ffff/0000/ffff$bel"
    Phase 'SET: old indexed rows #123456, ANSI magenta; truecolor unchanged'
    [Console]::Out.Write("$e]104;42;42$e\")
    $null = Query 'Selective reset 42' "$e]4;42;?$e\" $baseline42
    $null = Query 'Selective reset keeps 1' "$e]4;1;?$bel" "$e]4;1;rgb:ffff/0000/ffff$bel"
    Phase 'SELECTIVE RESET: indexed baseline restored, ANSI still magenta'
    [Console]::Out.Write("$e]104$bel")
    $null = Query 'Full reset 1' "$e]4;1;?$bel" $baseline1
    $null = Query 'Full reset 42' "$e]4;42;?$e\" $baseline42
    Phase 'FULL RESET: all original indexed colors restored'
} finally {
    [Console]::Out.Write("$e]104$bel$e[0m$e[?25h$e[9;1H")
    [Console]::Out.Flush()
    $null = [OscPaletteProbe]::SetConsoleMode($inputHandle, $inputMode)
    $null = [OscPaletteProbe]::SetConsoleMode($outputHandle, $outputMode)
    $report = [pscustomobject]@{
        Probe = 'OSC palette probe v1'
        OS = [Environment]::OSVersion.VersionString
        PowerShell = $PSVersionTable.PSVersion.ToString()
        TERM = $env:TERM
        TERM_PROGRAM = $env:TERM_PROGRAM
        Revision = (git rev-parse HEAD)
        BinarySHA256 = (Get-FileHash target/debug/harbor.exe -Algorithm SHA256).Hash
        ProbeSHA256 = (Get-FileHash $PSCommandPath -Algorithm SHA256).Hash
        ConPTY = (Get-Item target/debug/conpty/conpty.dll).VersionInfo.FileVersion
        Results = $results.ToArray()
        VisualAcceptance = 'NOT AUTOMATED: record named phase screenshots/observations separately'
        Exclusions = 'No WSL/SSH/tmux, native Unix, configuration reload, or real application acceptance beyond this named probe.'
    }
    if ($ReportPath) { $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8 }
    $results | Select-Object Name, Outcome | Format-Table
}
