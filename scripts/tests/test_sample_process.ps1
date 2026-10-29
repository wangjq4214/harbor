# Dependency-free Windows smoke checks; samples this test process, NOT Harbor.
$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot '../profiling/sample_process.ps1'
$samples = @(& $script -TargetPid $PID -DurationSeconds 1 -IntervalMs 100 -Phase sampler-test)
if ($samples.Count -lt 2) { throw 'Missing samples' }
if ($null -ne $samples[0].cpu_machine_percent) { throw 'First CPU delta must be unknown' }
for ($i = 1; $i -lt $samples.Count; $i++) {
    $before = $samples[$i - 1]
    $now = $samples[$i]
    if ($now.elapsed_seconds -le $before.elapsed_seconds) { throw 'Clock did not advance' }
    if ($now.private_bytes -le 0 -or $now.working_set_bytes -le 0) { throw 'Missing memory' }
    $expected = 100 * ($now.cpu_total_seconds - $before.cpu_total_seconds) /
        ($now.elapsed_seconds - $before.elapsed_seconds) / $now.logical_processors
    if ([Math]::Abs($now.cpu_machine_percent - $expected) -gt 0.000001) { throw 'Bad CPU delta' }
    if ($now.process_start_utc -ne $before.process_start_utc) { throw 'Identity changed' }
}
foreach ($arguments in @(
    @{ TargetPid = $PID; DurationSeconds = 1; IntervalMs = 1000; Phase = 'bad-interval' },
    @{ TargetPid = 0; Phase = 'bad-pid' },
    @{ TargetPid = $PID; Phase = 'contains whitespace' },
    @{ TargetPid = 2147483647; Phase = 'nonexistent' }
)) {
    $rejected = $false
    try { & $script @arguments | Out-Null } catch { $rejected = $true }
    if (-not $rejected) { throw 'Invalid arguments unexpectedly accepted' }
}
"PASS: sampler smoke checks ($($samples.Count) samples); not Harbor measurements"
