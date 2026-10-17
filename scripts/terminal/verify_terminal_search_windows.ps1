# Synthetic fixtures for manual terminal-search acceptance, not an acceptance runner.
# Run ONLY in a disposable Harbor tab: several modes erase/reset its terminal content.
# Does not change configuration, files, clipboard, or other application sessions.
[CmdletBinding()]
param(
    [ValidateSet('Static', 'History', 'Limit', 'Stream', 'Overwrite', 'Erase', 'Structural', 'Evict', 'PrefixEvict', 'Alternate', 'Reset')]
    [string]$Mode = 'Static',
    [ValidateRange(0, 120)]
    [int]$DelaySeconds = 10,
    [ValidateRange(1, 300)]
    [int]$DurationSeconds = 30
)

$ErrorActionPreference = 'Stop'
# Explicit UTF-8 also preserves Unicode when the fixture is captured through pipes.
# Restore the caller's console encoding even when a timed mode is interrupted.
$originalOutputEncoding = [Console]::OutputEncoding
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
try {
    $esc = [char]27
    $width = 80
    try {
        if ([Console]::WindowWidth -gt 8) { $width = [Console]::WindowWidth }
    } catch { }

    function Write-RetainedHistory {
        param([int]$Rows)
        for ($i = 0; $i -lt $Rows; $i++) {
            [Console]::Write("HISTORY_NEEDLE_{0:D4}`r`n", $i)
        }
    }

    [Console]::Write("`r`nSEARCH FIXTURE mode={0} initialColumns={1}`r`n", $Mode, $width)
    switch ($Mode) {
        'Static' {
            # The queried WRAPJOIN spans a soft wrap at the INITIAL width.
            [Console]::Write(('x' * ($width - 4)) + "WRAPJOIN`r`n")
            [Console]::Write("HARD`r`nJOIN`r`n`r`n")
            $unicode = 'UNICODE_' + [char]0x4E2D + ' e' + [char]0x0301 + ' ' +
                [char]::ConvertFromUtf32(0x1F469) + [char]0x200D +
                [char]::ConvertFromUtf32(0x1F4BB)
            [Console]::Write($unicode + "`r`n")
            # More than 16 default 1024-unit pages, with no explicit break inside.
            [Console]::Write(('p' * 8192) + 'PAGEEDGE' + ('q' * 8192) + "`r`n")
            [Console]::Write("STATIC_END`r`n")
        }
        'History' { Write-RetainedHistory 900 }
        'Limit' {
            for ($i = 0; $i -lt 400; $i++) {
                [Console]::Write("CAP_NEEDLE CAP_NEEDLE CAP_NEEDLE CAP_NEEDLE`r`n")
            }
        }
        'Stream' {
            Write-RetainedHistory 400
            # At 20 rows/second, default duration adds 600 rows. The requested retained
            # scope is not extended by the matcher; eviction near the end is legitimate.
            $clock = [Diagnostics.Stopwatch]::StartNew()
            $i = 0
            while ($clock.Elapsed.TotalSeconds -lt $DurationSeconds) {
                [Console]::Write("LIVE_TICK_{0:D5}`r`n", $i)
                $i++
                Start-Sleep -Milliseconds 50
            }
        }
        { $_ -in 'Overwrite', 'Erase', 'Structural', 'Reset' } {
            [Console]::Write("Arm search now; mutation in {0} seconds.`r`n", $DelaySeconds)
            [Console]::Write('OLD_NEEDLE')
            Start-Sleep -Seconds $DelaySeconds
            switch ($Mode) {
                'Overwrite' { [Console]::Write("`rNEW_VALUE_`r`n") }
                'Erase' { [Console]::Write("`r${esc}[2KERASED`r`n") }
                'Structural' { [Console]::Write("`r${esc}[1MSTRUCTURAL_EDIT`r`n") }
                'Reset' { [Console]::Write("${esc}cRESET_DONE`r`n") }
            }
        }
        'Evict' {
            [Console]::Write("Arm search now; eviction in {0} seconds.`r`nEVICT_NEEDLE`r`n", $DelaySeconds)
            Start-Sleep -Seconds $DelaySeconds
            for ($i = 0; $i -lt 2500; $i++) { [Console]::Write("FILL_{0:D5}`r`n", $i) }
            [Console]::Write("EVICTION_DONE`r`n")
        }
        'PrefixEvict' {
            [Console]::Write("Arm search now; long-line prefix eviction in {0} seconds.`r`n", $DelaySeconds)
            [Console]::Write('PREFIX_NEEDLE')
            Start-Sleep -Seconds $DelaySeconds
            [Console]::Write(('p' * ($width * 1200)) + "SURVIVOR_NEEDLE`r`n")
        }
        'Alternate' {
            [Console]::Write("Search primary now; alternate transition in {0} seconds.`r`nPRIMARY_NEEDLE`r`n", $DelaySeconds)
            Start-Sleep -Seconds $DelaySeconds
            [Console]::Write("${esc}[?1049hALT_NEEDLE`r`nResize now; primary returns after {0} seconds.`r`n", $DelaySeconds)
            try { Start-Sleep -Seconds $DelaySeconds }
            finally { [Console]::Write("${esc}[?1049l") }
            [Console]::Write("PRIMARY_RETURNED`r`n")
        }
    }
    [Console]::Write("FIXTURE_DONE`r`n")
} finally {
    [Console]::OutputEncoding = $originalOutputEncoding
}
