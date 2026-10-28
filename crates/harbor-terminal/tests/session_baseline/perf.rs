use super::*;

#[test]
#[ignore = "manual release-mode baseline; no PTY or GPU"]
fn one_and_eight_session_update_throughput() {
    for sessions in [1, 8] {
        black_box(sample(sessions)); // warmup excluded
        let mut samples: Vec<_> = (0..SAMPLES)
            .map(|_| sample(sessions).as_secs_f64())
            .collect();
        samples.sort_by(f64::total_cmp);
        println!(
            "sessions={sessions} steps/session={} updates/sample={} input_bytes/update={} median_seconds={:.6} updates_per_second={:.1} samples_seconds={samples:?}",
            STEPS_PER_SESSION,
            sessions * STEPS_PER_SESSION,
            OUTPUT.len(),
            samples[SAMPLES / 2],
            (sessions * STEPS_PER_SESSION) as f64 / samples[SAMPLES / 2],
        );
    }
}
