# Observes one explicit Windows PID. No launch, termination, payload or environment capture.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateRange(1, 2147483647)][int]$TargetPid,
    [ValidateRange(1, 600)][int]$DurationSeconds = 60,
    [ValidateRange(50, 5000)][int]$IntervalMs = 250,
    [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9_-]{1,64}$')][string]$Phase
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows PowerShell 7+ required.' }
if ($IntervalMs -ge $DurationSeconds * 1000) { throw 'Need at least two samples.' }
$process = Get-Process -Id $TargetPid -ErrorAction Stop
$identity = $process.StartTime.ToUniversalTime().ToString('o')
$processors = [Environment]::ProcessorCount
$clock = [Diagnostics.Stopwatch]::StartNew()
$previousSeconds = $null
$previousCpu = $null
try {
    while ($true) {
        $process.Refresh()
        if ($process.HasExited) { throw 'Target exited before capture completed; capture is incomplete.' }
        $seconds = $clock.Elapsed.TotalSeconds
        $cpu = $process.TotalProcessorTime.TotalSeconds
        $percent = $null
        if ($null -ne $previousSeconds) {
            $percent = 100 * ($cpu - $previousCpu) / ($seconds - $previousSeconds) / $processors
        }
        [pscustomobject]@{
            schema = 1
            phase = $Phase
            pid = $TargetPid
            process_start_utc = $identity
            utc = [DateTime]::UtcNow.ToString('o')
            elapsed_seconds = $seconds
            logical_processors = $processors
            cpu_total_seconds = $cpu
            cpu_machine_percent = $percent
            private_bytes = $process.PrivateMemorySize64
            working_set_bytes = $process.WorkingSet64
        }
        $previousSeconds = $seconds
        $previousCpu = $cpu
        if ($seconds -ge $DurationSeconds) { break }
        Start-Sleep -Milliseconds $IntervalMs
    }
} finally {
    $clock.Stop()
    $process.Dispose()
}
