<#
.SYNOPSIS
    Spec 0020 / ticket-set 0015 T0003 Harbor/ConPTY visual and copy workload v1.
.DESCRIPTION
    Run INSIDE a freshly built Harbor, using pwsh -NoLogo -NoProfile -File.
    Select/copy each A...B row with Harbor's copy command, then inspect the OS
    clipboard externally. This script never writes the clipboard or claims PASS.
    Optional file gates let an observer capture an already drawn base BEFORE
    releasing its suffix; delays alone do not establish separate PTY reads.
    Use a new, empty ControlDirectory. Gates time out after five minutes.
#>
[CmdletBinding()]
param([string]$ControlDirectory)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$e = [char]27
$heart = [string][char]0x2665
$vs15 = [string][char]0xFE0E
$vs16 = [string][char]0xFE0F
$zwj = [string][char]0x200D
$woman = [char]::ConvertFromUtf32(0x1F469)
$laptop = [char]::ConvertFromUtf32(0x1F4BB)
$missing = [char]::ConvertFromUtf32(0x10FFFF)
$acute = [string][char]0x0301
$bSentinel = 'B'

function Wait-Observer([string]$Stage) {
    if (-not $ControlDirectory) { return }
    [IO.File]::WriteAllText((Join-Path $ControlDirectory 'stage.txt'), $Stage)
    $gate = Join-Path $ControlDirectory "$Stage.continue"
    $deadline = [DateTime]::UtcNow.AddMinutes(5)
    while (-not (Test-Path -LiteralPath $gate)) {
        if ([DateTime]::UtcNow -ge $deadline) { throw "Observer gate timed out: $Stage" }
        Start-Sleep -Milliseconds 50
    }
}

if ($ControlDirectory) {
    if (-not (Test-Path -LiteralPath $ControlDirectory -PathType Container)) {
        throw 'ControlDirectory must be an existing, empty directory for this run.'
    }
    if (Get-ChildItem -LiteralPath $ControlDirectory -Force) {
        throw 'ControlDirectory must be empty; stale gates would invalidate fragmentation evidence.'
    }
}

[Console]::Out.Write("$e[0m$e[2J$e[H$e[?25l")
try {
    [Console]::Out.WriteLine('Spec0020 T0003 color emoji workload v1 (not a support verdict)')
    [Console]::Out.WriteLine("pwsh=$($PSVersionTable.PSVersion) OS=$([Environment]::OSVersion.Version)")
    [Console]::Out.WriteLine('0123456789012345678901234567890123456789')
    [Console]::Out.WriteLine("A$heart$vs16${bSentinel}")
    [Console]::Out.WriteLine("A$woman$zwj$laptop${bSentinel}")
    [Console]::Out.WriteLine("A$heart${bSentinel} A$heart$vs15${bSentinel}")
    [Console]::Out.WriteLine("$e[32;44mA$heart$vs16${bSentinel} A$woman$zwj$laptop${bSentinel}$e[0m")
    [Console]::Out.WriteLine("$e[30;47mA$heart$vs16${bSentinel} A$woman$zwj$laptop${bSentinel}$e[0m")
    [Console]::Out.WriteLine("A$woman$zwj$missing${bSentinel}")
    [Console]::Out.WriteLine("e${acute}X ${acute}X $e[4;53mA$woman$zwj$laptop${bSentinel}$e[0m")
    [Console]::Out.WriteLine("CONCEAL: A$e[8m$heart$vs16$e[28mB")
    # Deliberately no B/newline until the observer has captured the base.
    [Console]::Out.Write("A$heart")
    [Console]::Out.Flush()
    Wait-Observer 'heart-base'
    [Console]::Out.Write("$vs16${bSentinel}`r`nA$woman")
    [Console]::Out.Flush()
    Wait-Observer 'woman-base'
    [Console]::Out.Write("$zwj$laptop${bSentinel}`r`nEND - copy rows, resize/minimize/restore, compare neighbors.")
    [Console]::Out.Flush()
    Wait-Observer 'complete'
} finally {
    [Console]::Out.Write("$e[0m$e[?25h")
}
