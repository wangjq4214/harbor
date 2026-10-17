<#
.SYNOPSIS
    Windows runtime verification script for Unicode terminal text correctness (T0003 / Spec 0015).

.DESCRIPTION
    Runs all Unicode runtime verification fixtures sequentially in one go:
    1. Environment & console geometry snapshot.
    2. Visual fixtures with column rulers (combining marks, isolated mark with dotted circle, CJK mix, VS15/VS16, ZWJ emoji).
    3. PTY split-write simulation (delayed writes across PTY read boundaries).
    4. Right-edge wrapping and in-place overwrite/erase.
    5. Clipboard codepoint inspection and scalar integrity checks.
#>

[CmdletBinding()]
param(
    [switch]$NonInteractive
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)

# ANSI styles
$ESC = [char]0x1B
$RESET = "$ESC[0m"
$BOLD = "$ESC[1m"
$GREEN = "$ESC[32m"
$YELLOW = "$ESC[33m"
$CYAN = "$ESC[36m"
$RED = "$ESC[31m"
$GRAY = "$ESC[90m"

# Unicode constants constructed safely
$cCombiningAcute = [char]0x0301
$cDottedCircle   = [char]0x25CC
$cHeart          = [char]0x2665
$cVS15           = [char]0xFE0E
$cVS16           = [char]0xFE0F
$cZWJ            = [char]0x200D
$cCJK            = [char]0x754C  # CJK Ideograph U+754C
$sWoman          = [char]::ConvertFromUtf32(0x1F469) # U+1F469 Woman
$sLaptop         = [char]::ConvertFromUtf32(0x1F4BB) # U+1F4BB Laptop
$sWomanTech      = "$sWoman$cZWJ$sLaptop"             # Woman + ZWJ + Laptop

function Write-SectionHeader([string]$Title) {
    [Console]::Out.WriteLine("")
    [Console]::Out.WriteLine("$BOLD$CYAN=== $Title ===$RESET")
    [Console]::Out.WriteLine("")
}

function Write-Ruler([int]$Width = 40) {
    $tens = ""
    $units = ""
    for ($i = 0; $i -lt $Width; $i++) {
        if ($i % 10 -eq 0) {
            $tens += [string]($i / 10 % 10)
        } else {
            $tens += " "
        }
        $units += [string]($i % 10)
    }
    [Console]::Out.WriteLine("$GRAY$tens$RESET")
    [Console]::Out.WriteLine("$GRAY$units$RESET")
}

function Get-UnicodeCodePoints([string]$text) {
    $result = [System.Collections.Generic.List[string]]::new()
    for ($i = 0; $i -lt $text.Length; $i++) {
        if ([char]::IsHighSurrogate($text, $i) -and ($i + 1 -lt $text.Length) -and [char]::IsLowSurrogate($text, $i + 1)) {
            $cp = [char]::ConvertToUtf32($text, $i)
            $result.Add(("U+{0:X4}" -f $cp))
            $i++
        } else {
            $cp = [int][char]$text[$i]
            $result.Add(("U+{0:X4}" -f $cp))
        }
    }
    return $result
}

function Inspect-ClipboardText([string]$text) {
    if ([string]::IsNullOrEmpty($text)) {
        [Console]::Out.WriteLine("  $YELLOW[Clipboard is empty]$RESET")
        return
    }

    $trimmed = $text.TrimEnd("`r", "`n")
    $codepoints = Get-UnicodeCodePoints $trimmed

    [Console]::Out.WriteLine("  Raw String:     '$trimmed'")
    [Console]::Out.WriteLine("  Codepoint Count: $($codepoints.Count)")
    [Console]::Out.WriteLine("  Codepoints:     $($codepoints -join ' ')")

    # Invariant assertions
    if ($codepoints -contains "U+0301") {
        [Console]::Out.WriteLine("  $GREEN[PASS]$RESET Preserves U+0301 (Combining Acute Accent)")
    }
    if ($codepoints -contains "U+25CC") {
        [Console]::Out.WriteLine("  $RED[FAIL]$RESET Contains U+25CC (Dotted Circle)! Display cue must not be copied.")
    }
    if ($codepoints -contains "U+FE0E") {
        [Console]::Out.WriteLine("  $GREEN[PASS]$RESET Preserves U+FE0E (VS15 text presentation)")
    }
    if ($codepoints -contains "U+FE0F") {
        [Console]::Out.WriteLine("  $GREEN[PASS]$RESET Preserves U+FE0F (VS16 emoji presentation)")
    }
    if ($codepoints -contains "U+200D") {
        [Console]::Out.WriteLine("  $GREEN[PASS]$RESET Preserves U+200D (ZWJ joiner)")
    }
}

# ==============================================================================
# 1. Environment & Console Snapshot
# ==============================================================================
Write-SectionHeader "1. Environment & System Snapshot"

$gitRev = "Unknown"
$gitDirty = "Unknown"
try {
    $gitRev = (git rev-parse HEAD 2>$null).Trim()
    $status = (git status --porcelain 2>$null)
    if ([string]::IsNullOrWhiteSpace($status)) {
        $gitDirty = "Clean"
    } else {
        $gitDirty = "Dirty ($(($status | Measure-Object).Count) modified entries)"
    }
} catch {}

$osVersion = [System.Environment]::OSVersion.VersionString
$osCaption = "Windows"
try {
    $osCaption = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).Caption
} catch {}

$windowWidth = $Host.UI.RawUI.WindowSize.Width
$windowHeight = $Host.UI.RawUI.WindowSize.Height
$bufferWidth = $Host.UI.RawUI.BufferSize.Width
$bufferHeight = $Host.UI.RawUI.BufferSize.Height
$wtSession = $env:WT_SESSION
$termProg = $env:TERM_PROGRAM

[Console]::Out.WriteLine("  OS:                $osCaption ($osVersion)")
[Console]::Out.WriteLine("  PowerShell:        $($PSVersionTable.PSVersion)")
[Console]::Out.WriteLine("  Git Revision:      $gitRev")
[Console]::Out.WriteLine("  Git Tree Status:   $gitDirty")
[Console]::Out.WriteLine("  Terminal Size:     Columns=$windowWidth, Rows=$windowHeight (Buffer: ${bufferWidth}x${bufferHeight})")
[Console]::Out.WriteLine("  Terminal Host:     TERM_PROGRAM='$termProg', WT_SESSION='$wtSession'")


# ==============================================================================
# 2. Unicode Visual Presentation Fixtures
# ==============================================================================
Write-SectionHeader "2. Unicode Visual Presentation Fixtures"

[Console]::Out.WriteLine("Verify column alignment against the ruler below each test line.")
[Console]::Out.WriteLine("Expected: All sequences must occupy exact planned cells without cursor drift.`n")

# [2.1] Combining character (T0001)
[Console]::Out.WriteLine("$BOLD[2.1] Combining Mark (Base 'e' + U+0301 Combining Acute + 'X')$RESET")
[Console]::Out.WriteLine("Expected: 'e$cCombiningAcute' in Col 0, 'X' in Col 1 (Total width = 2 cells)")
Write-Ruler 20
[Console]::Out.WriteLine("e$cCombiningAcute`X`n")

# [2.2] Isolated combining mark at line start (T0001)
[Console]::Out.WriteLine("$BOLD[2.2] Isolated Combining Mark at Line Start (U+0301 + 'X')$RESET")
[Console]::Out.WriteLine("Expected: Dotted circle cue ($cDottedCircle$cCombiningAcute) in Col 0, 'X' in Col 1 (Total width = 2 cells)")
Write-Ruler 20
[Console]::Out.WriteLine("$cCombiningAcute`X`n")

# [2.3] Mixed ASCII, Combining, CJK Wide Character (T0001)
[Console]::Out.WriteLine("$BOLD[2.3] Mixed ASCII + Combining Mark + CJK ('0123456789' + e+U+0301 + '$cCJK')$RESET")
[Console]::Out.WriteLine("Expected: 10 ASCII (Cols 0-9) + 'e$cCombiningAcute' (Col 10) + '$cCJK' (Cols 11-12) (Total width = 13 cells)")
Write-Ruler 20
[Console]::Out.WriteLine("0123456789e$cCombiningAcute$cCJK`n")

# [2.4] Variation Selectors: Text vs Emoji presentation (T0002)
[Console]::Out.WriteLine("$BOLD[2.4] Variation Selectors: Text Presentation (VS15) vs Emoji Presentation (VS16)$RESET")
[Console]::Out.WriteLine("Expected: Text Heart (Col 0) + 'X' (Col 1); Emoji Heart (Cols 0-1) + 'X' (Col 2)")
Write-Ruler 20
[Console]::Out.WriteLine("$cHeart$cVS15`X  <- VS15 text: Heart(1 cell) + X")
[Console]::Out.WriteLine("$cHeart$cVS16`X <- VS16 emoji: Heart(2 cells) + X`n")

# [2.5] ZWJ Sequence (T0002)
[Console]::Out.WriteLine("$BOLD[2.5] ZWJ Sequence (Woman + ZWJ + Laptop + 'X')$RESET")
[Console]::Out.WriteLine("Expected: $sWomanTech occupies 2 columns (Cols 0-1), 'X' in Col 2. Fallback glyph is acceptable, width must be 2.")
Write-Ruler 20
[Console]::Out.WriteLine("$sWomanTech`X`n")


# ==============================================================================
# 3. PTY Split Write Simulation (Fragmented Reads)
# ==============================================================================
Write-SectionHeader "3. PTY Split Write Simulation (Fragmented Reads)"
[Console]::Out.WriteLine("Simulates network/PTY buffer fragmentation with 250ms delays between sequence components.")
[Console]::Out.WriteLine("Testing incremental state machine & dirty region repaints.`n")

$outStream = [Console]::OpenStandardOutput()

# Split 3.1: Combining mark split
[Console]::Out.Write("[3.1] Delayed Combining: e ... [250ms] ... U+0301 ... [250ms] ... X  ==> ")
$b1 = [System.Text.Encoding]::UTF8.GetBytes('e')
$outStream.Write($b1, 0, $b1.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$b2 = [System.Text.Encoding]::UTF8.GetBytes([string]$cCombiningAcute)
$outStream.Write($b2, 0, $b2.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$b3 = [System.Text.Encoding]::UTF8.GetBytes("X`r`n")
$outStream.Write($b3, 0, $b3.Length)
$outStream.Flush()

# Split 3.2: Variation selector split
[Console]::Out.Write("[3.2] Delayed VS16:      $cHeart ... [250ms] ... U+FE0F ... [250ms] ... X  ==> ")
$bHeart = [System.Text.Encoding]::UTF8.GetBytes([string]$cHeart)
$outStream.Write($bHeart, 0, $bHeart.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$bVS = [System.Text.Encoding]::UTF8.GetBytes([string]$cVS16)
$outStream.Write($bVS, 0, $bVS.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$bX = [System.Text.Encoding]::UTF8.GetBytes("X`r`n")
$outStream.Write($bX, 0, $bX.Length)
$outStream.Flush()

# Split 3.3: ZWJ sequence split
[Console]::Out.Write("[3.3] Delayed ZWJ:       $sWoman ... [250ms] ... ZWJ ... [250ms] ... $sLaptop ... [250ms] ... X ==> ")
$bWoman = [System.Text.Encoding]::UTF8.GetBytes($sWoman)
$outStream.Write($bWoman, 0, $bWoman.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$bZwj = [System.Text.Encoding]::UTF8.GetBytes([string]$cZWJ)
$outStream.Write($bZwj, 0, $bZwj.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$bLaptop = [System.Text.Encoding]::UTF8.GetBytes($sLaptop)
$outStream.Write($bLaptop, 0, $bLaptop.Length)
$outStream.Flush()
Start-Sleep -Milliseconds 250

$outStream.Write($bX, 0, $bX.Length)
$outStream.Flush()

[Console]::Out.WriteLine("")


# ==============================================================================
# 4. Edge Boundaries & In-place Overwrite/Erase
# ==============================================================================
Write-SectionHeader "4. Edge Boundaries & In-place Overwrite/Erase"

$width = [Math]::Max(20, $Host.UI.RawUI.WindowSize.Width)

# 4.1 Right Edge 2-cell wrap test
[Console]::Out.WriteLine("[4.1] Right Edge 2-cell Emoji Placement (Terminal width: $width cols)")
$padLength = $width - 2
$filler = "-" * $padLength
[Console]::Out.WriteLine("Writing $padLength dashes, then 2-cell ${sWomanTech}:")
[Console]::Out.Write("$filler$sWomanTech`r`n")
[Console]::Out.WriteLine("Expected: Wraps cleanly to next line without leaving orphan half-cells.`n")

# 4.2 Overwrite & Erase test
[Console]::Out.WriteLine("[4.2] In-place Overwrite and Erase to End-of-line (EL)")
[Console]::Out.WriteLine("Writing 'ABCDEFGHIJ' then overwriting with 'e$cCombiningAcute`X' + erase-to-EOL ($ESC[K):")
[Console]::Out.Write("ABCDEFGHIJ")
Start-Sleep -Milliseconds 300
[Console]::Out.Write("`re$cCombiningAcute`X$ESC[K`r`n")
[Console]::Out.WriteLine("Expected: Only 'e$cCombiningAcute`X' remains visible; columns 2-9 are erased.`n")


# ==============================================================================
# 5. Clipboard Scalar Codepoint Inspector
# ==============================================================================
Write-SectionHeader "5. Clipboard Scalar Codepoint Inspector"
[Console]::Out.WriteLine("Verifies that copied terminal text preserves exact raw codepoints (marks, VS, ZWJ)")
[Console]::Out.WriteLine("and does NOT copy display-only cues (such as dotted circle U+25CC).`n")

# Check current clipboard right now
[Console]::Out.WriteLine("$BOLD[Current Clipboard Content]$RESET")
$initialClip = $null
try {
    $initialClip = Get-Clipboard -Raw -ErrorAction SilentlyContinue
} catch {}
Inspect-ClipboardText $initialClip

if (-not $NonInteractive) {
    [Console]::Out.WriteLine("`n$BOLD[Interactive Clipboard Test]$RESET")
    [Console]::Out.WriteLine("Select and copy (Ctrl+Shift+C / Ctrl+C) any of the test lines above, then press [Enter].")
    [Console]::Out.WriteLine("(Press [Enter] to inspect, or 'q' to finish)")
    
    while ($true) {
        $key = Read-Host "Inspect clipboard [Enter] / Quit ['q']"
        if ($key -eq 'q') { break }

        $clip = $null
        try {
            $clip = Get-Clipboard -Raw -ErrorAction Stop
        } catch {
            [Console]::Out.WriteLine("  $RED Failed to read clipboard: $_$RESET")
            continue
        }
        Inspect-ClipboardText $clip
    }
}

Write-SectionHeader "Verification Run Completed"
