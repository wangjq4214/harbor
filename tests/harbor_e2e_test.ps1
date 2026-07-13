#Requires -Version 5.1
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Qa-q]$')]
    [string]$Section
)

$ESC = [char]27
$CSI = "$ESC["
$OnlySection = if ($Section) { $Section.ToUpperInvariant() } else { $null }
$env:GIT_PAGER = 'cat'

function Write-Raw([string]$Text) { [Console]::Write($Text) }
function Write-Line([string]$Text = '') { [Console]::WriteLine($Text) }
function Show-Section([string]$Id, [string]$Title) {
    Write-Line
    Write-Raw "${CSI}0m${CSI}1;36m"
    Write-Line ('═' * 50)
    Write-Line "  §$Id  $Title"
    Write-Line ('═' * 50)
    Write-Raw "${CSI}0m"
}
function Show-Info([string]$Text) { Write-Line "${CSI}90m  ▸ $Text${CSI}0m" }
function Show-Ok([string]$Text) { Write-Line "${CSI}32m  ✓ $Text${CSI}0m" }
function Show-Sep { Write-Line "${CSI}90m  $([string]::new('─', 34))${CSI}0m" }
function Wait-Section {
    if ([Console]::IsInputRedirected) { return $true }
    Write-Raw "`n${CSI}33m  [Press Enter to continue, or q to skip]${CSI}0m "
    $key = [Console]::ReadKey($true)
    Write-Line
    return $key.KeyChar -ne 'q'
}
function Should-Run([string]$Id) { return !$OnlySection -or $OnlySection -eq $Id }
function Repeat([string]$Text, [int]$Count) { Write-Raw ($Text * $Count) }
function Reset-Terminal { Write-Raw "${ESC}c" }
function Get-WindowWidth { if ([Console]::IsOutputRedirected) { return 80 }; return [Console]::WindowWidth }
function Get-WindowHeight { if ([Console]::IsOutputRedirected) { return 24 }; return [Console]::WindowHeight }
function Get-CursorRow { if ([Console]::IsOutputRedirected) { return 1 }; return [Math]::Max(1, [Console]::CursorTop + 1) }

if (Should-Run A) {
    Show-Section A 'Common Windows Commands (Basic Text Output)'
    Show-Info 'git config --global --list'; & git config --global --list 2>$null; if ($LASTEXITCODE) { Write-Line '  (no global Git configuration)' }; Show-Sep
    Show-Info 'git log --oneline --color -8'; & git log --oneline --color -8 2>$null; if ($LASTEXITCODE) { Write-Line '  (not a Git repository)' }; Show-Sep
    Show-Info 'git status --short'; & git status --short 2>$null; if ($LASTEXITCODE) { Write-Line '  (not a Git repository)' }; Show-Sep
    Show-Info 'git diff --stat HEAD~1 HEAD'; & git diff --stat 'HEAD~1' HEAD 2>$null; if ($LASTEXITCODE) { Write-Line '  (no previous commit)' }; Show-Sep
    Show-Info 'Get-ChildItem -Force'; Get-ChildItem -Force | Format-Table -AutoSize | Out-Host; Show-Sep
    Show-Info 'Environment variables (first 20)'; Get-ChildItem Env: | Sort-Object Name | Select-Object -First 20 | Format-Table -AutoSize | Out-Host; Show-Sep
    Show-Info 'Processes (first 10)'; Get-Process | Select-Object -First 10 Id, ProcessName, CPU | Format-Table -AutoSize | Out-Host; Show-Sep
    Show-Info 'Drives'; Get-PSDrive -PSProvider FileSystem | Format-Table -AutoSize | Out-Host; Show-Sep
    Show-Info 'Operating system version'; [Environment]::OSVersion.VersionString; Show-Sep
    Show-Ok '§A complete: verify text alignment, colors, and special characters'; [void](Wait-Section)
}

if (Should-Run B) {
    Show-Section B 'SGR Character Attributes'
    Show-Info 'Basic attributes'
    Write-Raw '  Normal  '; Write-Raw "${CSI}1mBold${CSI}0m  "; Write-Raw "${CSI}2mDim${CSI}0m  "; Write-Raw "${CSI}3mItalic${CSI}0m  "; Write-Raw "${CSI}4mUnderline${CSI}0m  "; Write-Raw "${CSI}5mBlink${CSI}0m  "; Write-Raw "${CSI}7mReverse${CSI}0m  "; Write-Line "${CSI}9mStrikethrough${CSI}0m"; Show-Sep
    Show-Info 'Attribute combinations'; Write-Line "  ${CSI}1;3mBold italic${CSI}0m  ${CSI}1;4mBold underline${CSI}0m  ${CSI}1;31mBold red${CSI}0m  ${CSI}3;4mItalic + underline${CSI}0m  ${CSI}1;3;4;31mAll combined${CSI}0m"; Show-Sep
    Show-Info 'reset: CSI 0 m is equivalent to CSI m'; Write-Line "  ${CSI}1;31mbold red → ${CSI}0m→CSI0m  ${CSI}1;31mbold red → ${CSI}m→CSIm"; Show-Sep
    Show-Info 'Multiple parameters and unknown SGR codes'; Write-Line "  ${CSI}0;1;31mbold red${CSI}0m  ${CSI}1;999;31mbold + unknown 999 + red${CSI}0m"; Show-Sep
    Show-Ok '§B complete'; [void](Wait-Section)
}

if (Should-Run C) {
    Show-Section C 'Color System'
    Show-Info 'Basic 16-color foreground (30-37, 90-97)'; foreach ($c in 30..37 + 90..97) { Write-Raw ("${CSI}{0}m {0,3} ${CSI}0m" -f $c) }; Write-Line; Show-Sep
    Show-Info 'Basic 16-color background (40-47, 100-107)'; foreach ($c in 40..47 + 100..107) { Write-Raw ("${CSI}{0}m {0,3} ${CSI}0m" -f $c) }; Write-Line; Show-Sep
    Show-Info '256-color system colors (0-15)'; foreach ($i in 0..15) { Write-Raw ("${CSI}38;5;{0}m{0,4}${CSI}0m" -f $i) }; Write-Line; Show-Sep
    Show-Info '256-color 6×6×6 color cube slice (16-87)'; foreach ($i in 16..87) { Write-Raw "${CSI}48;5;${i}m  ${CSI}0m"; if (($i - 15) % 36 -eq 0) { Write-Line } }; Show-Sep
    Show-Info '256-color grayscale (232-255)'; foreach ($i in 232..255) { Write-Raw "${CSI}48;5;${i}m  ${CSI}0m" }; Write-Line; Show-Sep
    Show-Info 'True Color foreground gradient'; foreach ($i in 0..71) { $r = 255 - $i * 3; $g = $i * 3; Write-Raw "${CSI}38;2;$r;$g;100m■${CSI}0m" }; Write-Line; Show-Sep
    Show-Info 'True Color background gradient'; foreach ($i in 0..71) { $b = 255 - $i * 3; $r = $i * 3; Write-Raw "${CSI}48;2;$r;50;${b}m  ${CSI}0m" }; Write-Line; Show-Sep
    Show-Ok '§C complete：verify smooth color transitions'; [void](Wait-Section)
}

if (Should-Run D) {
    Show-Section D 'Cursor Movement (CUU/CUD/CUF/CUB/CUP/CHA/VPA)'
    Show-Info 'CUU/CUD/CUF/CUB — relative movement'; Write-Line "  Line 1`n  Line 2`n  Line 3"; Write-Raw "${CSI}3A${CSI}4C●INS${CSI}3B"; Write-Line; Show-Sep
    Show-Info 'CUP — absolute positioning'; $row = Get-CursorRow; foreach ($col in 4, 12, 20, 28, 36) { Write-Raw "${CSI}${row};$($col+2)H★" }; Write-Line; Show-Sep
    Show-Info 'CHA (G) — absolute column positioning'; Write-Line '  XXXXXXXXXX'; Write-Raw "${CSI}1A${CSI}6G←col6${CSI}1B"; Write-Line; Show-Sep
    Show-Info 'VPA (d) — absolute row positioning'; Write-Line "`n`n"; Write-Raw "${CSI}3A${CSI}10d  VPA=10${CSI}3B"; Write-Line; Show-Sep
    Show-Info 'DECSC/DECRC — save and restore cursor'; Write-Raw "  [saved]${ESC}7${CSI}6Cmoved position${ESC}8←restored to [saved]"; Write-Line; Show-Sep
    Show-Info 'CSI s/u — alternate save/restore'; Write-Line "  ${CSI}s${CSI}5Cmoved${CSI}u←restored"; Show-Sep
    Show-Ok '§D complete'; [void](Wait-Section)
}

if (Should-Run E) {
    Show-Section E 'Erase Operations (ED / EL / ECH)'
    Show-Info 'EL 0 — cursor to end of line'; Write-Line '  AAAAABBBBBCCCCC'; Write-Line "${CSI}1A${CSI}8G${CSI}0K${CSI}1B  (Expected: blank after column 8)"; Show-Sep
    Show-Info 'EL 1 — start of line to cursor'; Write-Line '  AAAAABBBBBCCCCC'; Write-Line "${CSI}1A${CSI}8G${CSI}1K${CSI}1B  (Expected: columns 1-8 blank; CCCCC remains)"; Show-Sep
    Show-Info 'EL 2 — entire line'; Write-Line '  AAAAABBBBBCCCCC'; Write-Line "${CSI}1A${CSI}2K${CSI}1B  (Expected: entire lineblank)"; Show-Sep
    Show-Info 'ECH — erase N characters'; Write-Line '  ABCDEFGHIJ'; Write-Line "${CSI}1A${CSI}4G${CSI}4X${CSI}1B  (Expected: ABC    HIJ)"; Show-Sep
    Show-Info 'ED 0 — cursor to bottom of screen'; Write-Line "  First line`n  Second line`n  Third line"; Write-Line "${CSI}3A${CSI}6G${CSI}0J  (Expected: everything from the cursor to the bottom of the screen is cleared)"; Show-Sep
    Show-Info 'ED 1 — top of screen to cursor'; Write-Line "  First line`n  Second line`n  Third line"; Write-Line "${CSI}2A${CSI}1J${CSI}1B  (Expected: the first two lines are cleared; the third remains)"; Show-Sep
    Show-Ok '§E complete'; [void](Wait-Section)
}

if (Should-Run F) {
    Show-Section F 'Automatic Wrap (DECAWM) is equivalent to Pending Wrap'; $cols = [Math]::Max(20, (Get-WindowWidth))
    Show-Info 'DECAWM=on'; Write-Raw "${CSI}?7h  "; Repeat 'A' ($cols - 3); Write-Line "Z`nX  (X should be on a new line)"; Show-Sep
    Show-Info 'DECAWM=off'; Write-Raw "${CSI}?7l  "; Repeat 'B' ($cols - 3); Write-Line "OVERWRITE`n  (should overwrite the last column without wrapping)"; Write-Raw "${CSI}?7h"; Show-Sep
    Show-Info 'Pending Wrap + CR / CUP'; Write-Line '  1234567890'; Write-Line "${CSI}1A${CSI}10G!`r*${CSI}1B"; Write-Line '  AAAAAAAAAA'; Write-Line "${CSI}1A${CSI}10G!${CSI}1B  (positioning clears the pending-wrap state)"; Show-Sep
    Show-Ok '§F complete'; [void](Wait-Section)
}

if (Should-Run G) {
    Show-Section G 'Wide Characters (CJK Double-Width Characters)'
    Show-Info 'Basic wide-character rendering'; Write-Line "  中文：汉字测试渲染`n  韩文：안녕하세요`n  日文：ひらがなカタカナ`n  表情：🌈 🚀 ⭐ 🔥 ✓"; Show-Sep
    Show-Info 'Mixed wide-character and ASCII alignment'; Write-Line ('  {0,-10}| {1,-10}| {2,-6}' -f 'Name', 'Score', 'Grade'); Write-Line '  ----------+----------+------'; foreach ($v in @(@('张三', '99', 'A+'), @('Bob', '88', 'B'), @('李四五', '77', 'C'))) { Write-Line ('  {0,-10}| {1,-10}| {2,-6}' -f $v) }; Show-Sep
    Show-Info 'Wide-character wrapping at the right margin'; Write-Line "  xxxxxxxxxxxx中"; Write-Line "  (Expected: '中' occupies two columns at the line end, then wraps correctly)"; Show-Sep
    Show-Info 'Wide-character continuation-cell fallback'; Write-Line '  中文字'; Write-Line "${CSI}1A${CSI}3G`bX${CSI}1B  (Expected: the entire wide character is replaced without breaking alignment)"; Show-Sep
    Show-Ok '§G complete'; [void](Wait-Section)
}

if (Should-Run H) {
    Show-Section H 'DEC Special Graphics Character Set'
    Show-Info 'G0=DEC Special Graphics'; Write-Line "${ESC}(0  lqqqqqqqqqqqqqqqk`n  x  Harbor Term   x`n  tqqqqqqqqqqqqqqqu`n  x  DEC Graphics  x`n  mqqqqqqqqqqqqqqqj${ESC}(B"; Show-Sep
    Show-Info 'Character mapping a-z'; Write-Line "  ASCII: abcdefghijklmnopqrstuvwxyz`n  DEC:   ${ESC}(0abcdefghijklmnopqrstuvwxyz${ESC}(B"; Show-Sep
    Show-Info 'G1 / SO-SI'; Write-Line "${ESC})0  ASCII(G0): lqk`n  DEC(G1):   $([char]14)lqk$([char]15)"; Show-Sep
    Show-Ok '§H complete'; [void](Wait-Section)
}

if (Should-Run I) {
    Show-Section I 'Scrolling Region (DECSTBM) is equivalent to SU/SD/IL/DL'; Write-Raw "${CSI}2J${CSI}H"
    foreach ($i in 1..15) { Write-Line ('  Line {0:D2}: {1}' -f $i, ('─' * 30)) }
    Show-Info 'Region [4,10], SU 2 lines'; Write-Line "${CSI}4;10r${CSI}10;1H${CSI}2S${CSI}r${CSI}17;1H  Expected: lines 4-10 scroll up 2 lines"; Show-Info 'SD 1 line'; Write-Line "${CSI}4;10r${CSI}4;1H${CSI}1T${CSI}r${CSI}18;1H  Expected: Regionscrolls down 1 line"; Show-Info 'IL / DL'; Write-Line "${CSI}5;1H${CSI}2L${CSI}19;1H  insert 2 lines${CSI}5;1H${CSI}2M${CSI}20;1H  delete 2 lines"; Show-Sep
    Show-Ok '§I complete'; [void](Wait-Section); Reset-Terminal
}

if (Should-Run J) {
    Show-Section J 'Horizontal Margins (DECLRMM / DECSLRM)'; Write-Raw "${CSI}2J${CSI}H${CSI}?69h${CSI}5;40s${CSI}2;5H"
    Repeat 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456' 6; Write-Raw "${CSI}4;20H${CSI}100DL${CSI}5;20H${CSI}2K${CSI}?69l${CSI}r${CSI}8;1H"; Write-Line "`n  Expected: text wraps within col 5-40 "; Show-Sep
    Show-Ok '§J complete'; [void](Wait-Section); Reset-Terminal
}

if (Should-Run K) {
    Show-Section K 'Alternate Screen'; Show-Info 'Switching to the alternate screen; press any key to return'
    Start-Sleep -Milliseconds 500; Write-Raw "${CSI}?1049h${CSI}2J${CSI}H${CSI}1;32m"; Write-Line "╔═══════════════════════════════════╗`n║      Alternate Screen      ║`n║ The primary screen should be fully restored        ║`n║ Press any key to return to the primary screen...               ║`n╚═══════════════════════════════════╝${CSI}0m"; if (![Console]::IsInputRedirected) { [void][Console]::ReadKey($true) }; Write-Raw "${CSI}?1049l"; Show-Sep
    Show-Ok '§K complete'; [void](Wait-Section)
}

if (Should-Run L) {
    Show-Section L 'Tab Stops (HTS / TBC / HT)'; Show-Info 'Default tab stops'; Write-Line "  col:1`t9`t17`t25`t33"; Show-Sep
    Show-Info 'Custom tab stops col 6,14,24'; Write-Line "${CSI}1;6H${ESC}H${CSI}1;14H${ESC}H${CSI}1;24H${ESC}H${CSI}1;1H  >`t>`t>`t>"; Show-Info 'TBC 0 / 3'; Write-Line "${CSI}1;14H${CSI}0g${CSI}1;1H  >`t>`t>${CSI}3g"; foreach ($col in 9, 17, 25, 33, 41, 49, 57, 65, 73) { Write-Raw "${CSI}1;${col}H${ESC}H" }; Show-Sep
    Show-Ok '§L complete'; [void](Wait-Section)
}

if (Should-Run M) {
    Show-Section M 'Soft Reset (DECSTR) / Hard Reset (RIS)'; Show-Info 'Set multiple attributes'; Write-Line "${CSI}1;31m${CSI}?7l${CSI}?25l${CSI}?1h${CSI}2;5r  Set: bold red / no wrap / hidden cursor / application cursor keys / scrolling region"; Start-Sleep -Milliseconds 500
    Show-Info 'DECSTR Soft Reset'; Write-Line "${CSI}!p  Soft reset restores defaults without clearing the screen"; Show-Info 'RIS hard reset (in 2 seconds)'; Start-Sleep -Seconds 2; Reset-Terminal; Write-Line '  Hard reset complete'; Show-Sep
    Show-Ok '§M complete'; [void](Wait-Section)
}

if (Should-Run N) {
    Show-Section N 'Application Cursor Keys (DECCKM) is equivalent toModifier Keys'; Show-Info 'Normal/application cursor mode'; Write-Line "${CSI}?1l  Normal: \x1b[A \x1b[B \x1b[C \x1b[D`n${CSI}?1h  Application: \x1bOA \x1bOB \x1bOC \x1bOD${CSI}?1l"; Show-Sep
    Show-Info 'ANSI modifier encoding'; foreach ($v in @(@('Shift+↑', '\x1b[1;2A'), @('Alt+↑', '\x1b[1;3A'), @('Ctrl+↑', '\x1b[1;5A'), @('Ctrl+Home', '\x1b[1;5H'), @('Ctrl+Delete', '\x1b[3;5~'), @('Shift+Tab', '\x1b[Z'), @('Alt+a', '\x1ba'))) { Write-Line ('  {0,-20} {1}' -f $v) }; Show-Sep
    Show-Info 'Application keypad mode'; Write-Line "  ${ESC}=  → DECKPAM`n  Enter=\x1bOM  0=\x1bOp  1=\x1bOq  +=\x1bOk`n  ${ESC}>  → DECKPNM"; Write-Raw "${ESC}>"; Show-Sep
    Show-Ok '§N complete'; [void](Wait-Section)
}

if (Should-Run O) {
    Show-Section O 'Insert/Delete Characters (ICH / DCH / ECH / REP)'
    foreach ($test in @(@('ICH', '3@', 'ABC   DEFGHI'), @('DCH', '3P', 'ABCGHIJ'), @('ECH', '3X', 'ABC   GHIJ'))) { Show-Info $test[0]; Write-Line '  ABCDEFGHIJ'; Write-Line "${CSI}1A${CSI}4G${CSI}$($test[1])${CSI}1B  (Expected: $($test[2]))"; Show-Sep }
    Show-Info 'REP'; Write-Line "  A${CSI}19b`n  Expected: 20 A characters"; Show-Info 'REP Wide Characters'; Write-Line "  中${CSI}4b"; Show-Sep
    Show-Ok '§O complete'; [void](Wait-Section)
}

if (Should-Run P) {
    Show-Section P 'Rectangular Area Operations (DEC Extension §32)'; Write-Raw "${CSI}2J${CSI}H"; foreach ($i in 1..12) { Write-Raw ("${CSI}{0};1H  Line {0:D2}: {1}" -f $i, ('#' * 50)) }
    Show-Info 'DECERA [3,10]-[6,30]'; Write-Line "${CSI}3;10;6;30`$z${CSI}14;1H"; Show-Info 'DECFRA fill *'; Write-Line "${CSI}42;3;10;5;20`$x${CSI}15;1H"; Show-Info 'DECCRA copy'; Write-Line "${CSI}3;10;5;20;;8;35`$v${CSI}16;1H"; Show-Info 'DECCARA / DECRARA'; Write-Line "${CSI}3;10;3;20;1`$r${CSI}3;10;3;20;1`$t${CSI}18;1H"; [void](Wait-Section); Reset-Terminal; Show-Ok '§P complete'
}

if (Should-Run Q) {
    Show-Section Q 'Character Protection (DECSCA) and Selective Erase'; Write-Raw "${CSI}2J${CSI}H  Writing a mix of protected and unprotected characters`n`n  ${CSI}0`"qa${CSI}1`"q${CSI}1;31mB${CSI}0m${CSI}0`"qc${CSI}1`"q${CSI}1;31mD${CSI}0m${CSI}0`"qe`n`n"
    Show-Info 'DECSEL 2 — protected characters remain'; Write-Line "${CSI}3;1H${CSI}?2K${CSI}5;1H  Expected: a/c/e cleared; B/D remain"; Show-Info 'DECSED 2 — selective full-screen erase'; foreach ($row in 7, 8, 9) { Write-Raw "${CSI}${row};1H  ${CSI}0`"qnormal${CSI}1`"q${CSI}1;34mPROT${CSI}0m${CSI}0`"qnormal" }; Write-Line "${CSI}11;1H${CSI}?2J${CSI}12;1H  Expected: normal cleared; PROT remain"; Show-Sep
    Show-Ok '§Q complete'; [void](Wait-Section)
}

Write-Line
Write-Line "${CSI}1;32m╔═══════════════════════════════════════════════╗"
Write-Line '║        harbor end-to-end test script complete          ║'
Write-Line '║  All selected sections ran; visually verify the terminal output     ║'
Write-Line "╚═══════════════════════════════════════════════╝${CSI}0m"
Write-Line ("  Time: {0:yyyy-MM-dd HH:mm:ss}" -f (Get-Date))
Write-Line "  Size: $(Get-WindowWidth)×$(Get-WindowHeight) (cols×rows)"
$termName = if ($env:TERM) { $env:TERM } else { 'unknown' }
Write-Line "  TERM: $termName"
