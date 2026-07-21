use crate::font::FontBook;
use harbor_config::TEXT_PADDING;
use harbor_types::{DirtyRange, TerminalSize};
use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

/// GPU layer whose upload work is included in the render profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderLayer {
    Background,
    Text,
    Decoration,
    Selection,
    Cursor,
    Scrollbar,
}

impl RenderLayer {
    pub const COUNT: usize = 6;

    const fn index(self) -> usize {
        match self {
            Self::Background => 0,
            Self::Text => 1,
            Self::Decoration => 2,
            Self::Selection => 3,
            Self::Cursor => 4,
            Self::Scrollbar => 5,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Text => "text",
            Self::Decoration => "decoration",
            Self::Selection => "selection",
            Self::Cursor => "cursor",
            Self::Scrollbar => "scrollbar",
        }
    }
}

/// Upload operation selected for a dirty grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadMode {
    None,
    Incremental,
    Full,
}

/// Pure upload decision, separated from wgpu so it can be tested headlessly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPlan {
    pub mode: UploadMode,
    pub dirty_range_count: usize,
    pub dirty_cells: usize,
    pub dirty_bytes: usize,
    pub full_bytes: usize,
}

/// Chooses full writes when fragmented or broad damage makes them cheaper.
#[derive(Clone, Copy, Debug)]
pub struct UploadPolicy {
    full_upload_ratio: f64,
    max_incremental_ranges: usize,
}

impl Default for UploadPolicy {
    fn default() -> Self {
        Self {
            full_upload_ratio: 0.5,
            max_incremental_ranges: 64,
        }
    }
}

impl UploadPolicy {
    pub fn decide(
        self,
        rows: usize,
        cols: usize,
        bytes_per_cell: usize,
        dirty_ranges: &[DirtyRange],
        force_full: bool,
    ) -> UploadPlan {
        let dirty_cells = dirty_ranges.iter().fold(0usize, |total, range| {
            total.saturating_add(range.end_col.saturating_sub(range.start_col))
        });
        let dirty_bytes = dirty_cells.saturating_mul(bytes_per_cell);
        let full_bytes = rows.saturating_mul(cols).saturating_mul(bytes_per_cell);
        if force_full {
            return UploadPlan {
                mode: UploadMode::Full,
                dirty_range_count: dirty_ranges.len(),
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        if dirty_ranges.is_empty() {
            return UploadPlan {
                mode: UploadMode::None,
                dirty_range_count: 0,
                dirty_cells,
                dirty_bytes,
                full_bytes,
            };
        }
        let ratio = if full_bytes == 0 {
            1.0
        } else {
            dirty_bytes as f64 / full_bytes as f64
        };
        let mode = if ratio >= self.full_upload_ratio
            || dirty_ranges.len() > self.max_incremental_ranges
        {
            UploadMode::Full
        } else {
            UploadMode::Incremental
        };
        UploadPlan {
            mode,
            dirty_range_count: dirty_ranges.len(),
            dirty_cells,
            dirty_bytes,
            full_bytes,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LayerMetrics {
    pub dirty_ranges: u64,
    pub dirty_bytes: u64,
    pub max_dirty_ratio_milli: u16,
    pub upload_calls: u64,
    pub upload_bytes: u64,
    pub full_rebuilds: u64,
    pub incremental_rebuilds: u64,
    pub glyph_misses: u64,
    pub glyph_upload_calls: u64,
    pub glyph_upload_bytes: u64,
    pub atlas_evictions: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderMetricsSnapshot {
    pub enabled: bool,
    pub backend: String,
    pub hardware_class: String,
    pub frame_count: u64,
    pub frame_budget_misses: u64,
    pub mailbox_overwrites: u64,
    pub coalesced_updates: u64,
    pub revision_lag_total: u64,
    pub revision_lag_max: u64,
    pub snapshot_build_count: u64,
    pub command_ack_count: u64,
    pub frame_p95: Duration,
    pub frame_p99: Duration,
    pub input_ack_p95: Duration,
    pub input_ack_p99: Duration,
    pub snapshot_build_p95: Duration,
    pub prepare_p95: Duration,
    pub encode_p95: Duration,
    pub present_p95: Duration,
    pub present_interval_p95: Duration,
    pub layers: [LayerMetrics; RenderLayer::COUNT],
}

#[derive(Default)]
struct MetricsState {
    backend: String,
    hardware_class: String,
    frame_count: u64,
    frame_budget_misses: u64,
    mailbox_overwrites: u64,
    coalesced_updates: u64,
    revision_lag_total: u64,
    revision_lag_max: u64,
    snapshot_build_count: u64,
    command_ack_count: u64,
    frame_samples: Vec<Duration>,
    input_ack_samples: Vec<Duration>,
    snapshot_build_samples: Vec<Duration>,
    prepare_samples: Vec<Duration>,
    encode_samples: Vec<Duration>,
    present_samples: Vec<Duration>,
    present_interval_samples: Vec<Duration>,
    layers: [LayerMetrics; RenderLayer::COUNT],
}

/// Thread-safe, bounded render profile collector.
pub struct RenderMetrics {
    enabled: bool,
    state: Mutex<MetricsState>,
}

impl Default for RenderMetrics {
    fn default() -> Self {
        Self::new(true)
    }
}

impl RenderMetrics {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            state: Mutex::new(MetricsState::default()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_backend_hardware(
        &self,
        backend: impl Into<String>,
        hardware_class: impl Into<String>,
    ) {
        if !self.enabled {
            return;
        }
        let mut state = self.lock();
        state.backend = backend.into();
        state.hardware_class = hardware_class.into();
    }

    pub fn record_upload_plan(&self, layer: RenderLayer, plan: UploadPlan) {
        if !self.enabled {
            return;
        }
        let layer = &mut self.lock().layers[layer.index()];
        layer.dirty_ranges = layer
            .dirty_ranges
            .saturating_add(plan.dirty_range_count as u64);
        layer.dirty_bytes = layer.dirty_bytes.saturating_add(plan.dirty_bytes as u64);
        let ratio_milli = if plan.full_bytes == 0 {
            0
        } else {
            ((plan.dirty_bytes as u64)
                .saturating_mul(1_000)
                .checked_div(plan.full_bytes as u64)
                .unwrap_or(0))
            .min(u16::MAX as u64) as u16
        };
        layer.max_dirty_ratio_milli = layer.max_dirty_ratio_milli.max(ratio_milli);
        match plan.mode {
            UploadMode::None => {}
            UploadMode::Incremental => layer.incremental_rebuilds += 1,
            UploadMode::Full => layer.full_rebuilds += 1,
        }
    }

    pub fn record_upload(&self, layer: RenderLayer, bytes: usize) {
        if !self.enabled {
            return;
        }
        let layer = &mut self.lock().layers[layer.index()];
        layer.upload_calls += 1;
        layer.upload_bytes = layer.upload_bytes.saturating_add(bytes as u64);
    }

    pub fn record_glyph_upload(&self, bytes: usize) {
        if !self.enabled {
            return;
        }
        let layer = &mut self.lock().layers[RenderLayer::Text.index()];
        layer.glyph_upload_calls = layer.glyph_upload_calls.saturating_add(1);
        layer.glyph_upload_bytes = layer.glyph_upload_bytes.saturating_add(bytes as u64);
    }

    pub fn record_glyphs(&self, misses: usize, evicted: bool) {
        if !self.enabled {
            return;
        }
        let layer = &mut self.lock().layers[RenderLayer::Text.index()];
        layer.glyph_misses = layer.glyph_misses.saturating_add(misses as u64);
        if evicted {
            layer.atlas_evictions += 1;
        }
    }

    pub fn record_snapshot_build(&self, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        let mut state = self.lock();
        state.snapshot_build_count += 1;
        push_sample(&mut state.snapshot_build_samples, elapsed);
    }

    pub fn record_mailbox(&self, overwrite: bool, revision_lag: u64) {
        if !self.enabled {
            return;
        }
        let mut state = self.lock();
        if overwrite {
            state.mailbox_overwrites += 1;
        }
        state.revision_lag_total = state.revision_lag_total.saturating_add(revision_lag);
        state.revision_lag_max = state.revision_lag_max.max(revision_lag);
    }
    pub fn record_coalesced_updates(&self, count: usize) {
        if !self.enabled || count <= 1 {
            return;
        }
        let mut state = self.lock();
        state.coalesced_updates = state.coalesced_updates.saturating_add((count - 1) as u64);
    }

    pub fn record_command_ack(&self, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        let mut state = self.lock();
        state.command_ack_count += 1;
        push_sample(&mut state.input_ack_samples, elapsed);
    }

    pub fn record_frame(
        &self,
        frame: Duration,
        encode: Duration,
        present_interval: Option<Duration>,
    ) -> u64 {
        if !self.enabled {
            return 0;
        }
        let mut state = self.lock();
        state.frame_count += 1;
        if frame > Duration::from_nanos(16_666_667) {
            state.frame_budget_misses += 1;
        }
        push_sample(&mut state.frame_samples, frame);
        push_sample(&mut state.encode_samples, encode);
        if let Some(interval) = present_interval {
            push_sample(&mut state.present_interval_samples, interval);
        }
        state.frame_count
    }

    pub fn record_present(&self, elapsed: Duration) {
        if self.enabled {
            push_sample(&mut self.lock().present_samples, elapsed);
        }
    }
    pub fn record_prepare(&self, elapsed: Duration) {
        if self.enabled {
            push_sample(&mut self.lock().prepare_samples, elapsed);
        }
    }

    pub fn snapshot(&self) -> RenderMetricsSnapshot {
        let state = self.lock();
        RenderMetricsSnapshot {
            enabled: self.enabled,
            backend: state.backend.clone(),
            hardware_class: state.hardware_class.clone(),
            frame_count: state.frame_count,
            frame_budget_misses: state.frame_budget_misses,
            mailbox_overwrites: state.mailbox_overwrites,
            revision_lag_total: state.revision_lag_total,
            revision_lag_max: state.revision_lag_max,
            coalesced_updates: state.coalesced_updates,
            snapshot_build_count: state.snapshot_build_count,
            command_ack_count: state.command_ack_count,
            frame_p95: percentile(&state.frame_samples, 0.95),
            frame_p99: percentile(&state.frame_samples, 0.99),
            input_ack_p95: percentile(&state.input_ack_samples, 0.95),
            input_ack_p99: percentile(&state.input_ack_samples, 0.99),
            snapshot_build_p95: percentile(&state.snapshot_build_samples, 0.95),
            prepare_p95: percentile(&state.prepare_samples, 0.95),
            encode_p95: percentile(&state.encode_samples, 0.95),
            present_p95: percentile(&state.present_samples, 0.95),
            present_interval_p95: percentile(&state.present_interval_samples, 0.95),
            layers: state.layers.clone(),
        }
    }

    /// U6 remains deferred until a profile demonstrates a specific layout bottleneck.
    pub fn profile_report(&self) -> String {
        let snapshot = self.snapshot();
        let layers = [
            RenderLayer::Background,
            RenderLayer::Text,
            RenderLayer::Decoration,
            RenderLayer::Selection,
            RenderLayer::Cursor,
            RenderLayer::Scrollbar,
        ]
        .into_iter()
        .map(|layer| {
            let metrics = &snapshot.layers[layer.index()];
            format!(
                "{}:ranges={},dirty_bytes={},max_dirty_ratio_milli={},uploads={},upload_bytes={},full={},incremental={},glyph_misses={},atlas_evictions={},glyph_uploads={},glyph_upload_bytes={}",
                layer.name(),
                metrics.dirty_ranges,
                metrics.dirty_bytes,
                metrics.max_dirty_ratio_milli,
                metrics.upload_calls,
                metrics.upload_bytes,
                metrics.full_rebuilds,
                metrics.incremental_rebuilds,
                metrics.glyph_misses,
                metrics.atlas_evictions,
                metrics.glyph_upload_calls,
                metrics.glyph_upload_bytes,
            )
        })
        .collect::<Vec<_>>()
        .join(";");
        format!(
            "render_profile backend={} hardware={} workload_matrix=unqualified \
frame_count={} frame_budget_misses={} frame_p95_ms={:.3} frame_p99_ms={:.3} \
snapshot_build_count={} snapshot_build_p95_ms={:.3} prepare_p95_ms={:.3} encode_p95_ms={:.3} \
present_p95_ms={:.3} present_interval_p95_ms={:.3} mailbox_overwrites={} coalesced_updates={} revision_lag_total={} revision_lag_max={} \
command_ack_count={} input_ack_p95_ms={:.3} input_ack_p99_ms={:.3} layers={} gate_decision=deferred",
            snapshot.backend,
            snapshot.hardware_class,
            snapshot.frame_count,
            snapshot.frame_budget_misses,
            millis(snapshot.frame_p95),
            millis(snapshot.frame_p99),
            snapshot.snapshot_build_count,
            millis(snapshot.snapshot_build_p95),
            millis(snapshot.prepare_p95),
            millis(snapshot.encode_p95),
            millis(snapshot.present_p95),
            millis(snapshot.present_interval_p95),
            snapshot.mailbox_overwrites,
            snapshot.coalesced_updates,
            snapshot.revision_lag_total,
            snapshot.revision_lag_max,
            snapshot.command_ack_count,
            millis(snapshot.input_ack_p95),
            millis(snapshot.input_ack_p99),
            layers,
        )
    }

    fn lock(&self) -> MutexGuard<'_, MetricsState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn push_sample(samples: &mut Vec<Duration>, sample: Duration) {
    const MAX_SAMPLES: usize = 4096;
    if samples.len() == MAX_SAMPLES {
        samples.remove(0);
    }
    samples.push(sample);
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut values = samples.to_vec();
    values.sort_unstable();
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

/// Fixed measurements used to map window pixels to terminal cells.
#[derive(Clone, Copy)]
pub struct TextMetrics {
    pub cell_width: f32,
    pub line_height: f32,
    pub ascent: f32,
    /// Distance from cell top to underline top edge (px).
    pub underline_position: f32,
    pub underline_thickness: f32,
    /// Distance from cell top to strikethrough center (px).
    pub strikethrough_position: f32,
    pub strikethrough_thickness: f32,
}

impl TextMetrics {
    pub fn new(fonts: &FontBook) -> Self {
        let (cell_width, line_height, ascent) = fonts.terminal_metrics();
        let (underline_position, strikethrough_position) = fonts
            .primary_horizontal_line_metrics(harbor_config::FONT_SIZE)
            .map(|lm| {
                let d = lm.descent.abs();
                (line_height - d + 1.0, (line_height - d) * 0.45)
            })
            .unwrap_or((line_height * 0.8, line_height * 0.45));

        Self {
            cell_width,
            line_height,
            ascent,
            underline_position,
            underline_thickness: 1.5,
            strikethrough_position,
            strikethrough_thickness: 1.5,
        }
    }

    pub fn terminal_size(self, width: u32, height: u32) -> TerminalSize {
        let text_width = (width as f32 - TEXT_PADDING * 2.0).max(self.cell_width);
        let text_height = (height as f32 - TEXT_PADDING * 2.0).max(self.line_height);

        TerminalSize {
            rows: (text_height / self.line_height).floor().max(1.0) as usize,
            cols: (text_width / self.cell_width).floor().max(1.0) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(row: usize, start_col: usize, end_col: usize) -> DirtyRange {
        DirtyRange {
            row,
            start_col,
            end_col,
        }
    }

    #[test]
    fn upload_policy_preserves_sparse_ranges_and_switches_for_broad_damage() {
        let policy = UploadPolicy::default();
        let sparse = policy.decide(10, 10, 4, &[range(2, 1, 2)], false);
        assert_eq!(sparse.mode, UploadMode::Incremental);
        assert_eq!(sparse.dirty_range_count, 1);
        assert_eq!(sparse.dirty_cells, 1);
        assert_eq!(sparse.dirty_bytes, 4);

        let broad = policy.decide(
            10,
            10,
            4,
            &[
                range(0, 0, 10),
                range(1, 0, 10),
                range(2, 0, 10),
                range(3, 0, 10),
                range(4, 0, 10),
            ],
            false,
        );
        assert_eq!(broad.mode, UploadMode::Full);
        assert_eq!(broad.full_bytes, 400);
    }

    #[test]
    fn upload_policy_can_force_full_after_a_revision_gap() {
        let plan = UploadPolicy::default().decide(2, 2, 8, &[range(1, 1, 2)], true);
        assert_eq!(plan.mode, UploadMode::Full);
        assert_eq!(plan.dirty_range_count, 1);
        assert_eq!(plan.full_bytes, 32);
    }

    #[test]
    fn disabled_metrics_do_not_collect_or_change_decisions() {
        let metrics = RenderMetrics::new(false);
        let plan = UploadPolicy::default().decide(1, 2, 4, &[range(0, 0, 1)], false);
        metrics.record_upload_plan(RenderLayer::Text, plan);
        metrics.record_upload(RenderLayer::Text, 4);
        metrics.record_glyphs(2, true);
        metrics.record_frame(Duration::from_millis(20), Duration::from_millis(1), None);
        let snapshot = metrics.snapshot();
        assert!(!snapshot.enabled);
        assert_eq!(snapshot.frame_count, 0);
        assert_eq!(
            snapshot.layers[RenderLayer::Text.index()],
            LayerMetrics::default()
        );
    }

    #[test]
    fn metrics_separate_snapshot_prepare_and_frame_samples() {
        let metrics = RenderMetrics::default();
        metrics.record_snapshot_build(Duration::from_millis(3));
        metrics.record_prepare(Duration::from_millis(7));
        metrics.record_present(Duration::from_millis(1));
        metrics.record_frame(
            Duration::from_millis(8),
            Duration::from_millis(2),
            Some(Duration::from_millis(16)),
        );
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.snapshot_build_count, 1);
        assert_eq!(snapshot.snapshot_build_p95, Duration::from_millis(3));
        assert_eq!(snapshot.prepare_p95, Duration::from_millis(7));
        assert_eq!(snapshot.frame_p95, Duration::from_millis(8));
        assert_eq!(snapshot.present_p95, Duration::from_millis(1));
        assert_eq!(snapshot.present_interval_p95, Duration::from_millis(16));
    }
    #[test]
    fn metrics_report_mailbox_ack_upload_and_gate_fields() {
        let metrics = RenderMetrics::default();
        let plan = UploadPolicy::default().decide(2, 2, 16, &[range(0, 0, 1)], false);
        metrics.record_upload_plan(RenderLayer::Text, plan);
        metrics.record_glyph_upload(24);
        metrics.record_upload(RenderLayer::Text, 16);
        metrics.record_glyphs(3, true);
        metrics.record_mailbox(true, 2);
        metrics.record_coalesced_updates(3);
        metrics.record_command_ack(Duration::from_millis(4));
        let snapshot = metrics.snapshot();
        let text = &snapshot.layers[RenderLayer::Text.index()];
        assert_eq!(text.glyph_upload_calls, 1);
        assert_eq!(text.glyph_upload_bytes, 24);
        assert_eq!(text.dirty_ranges, 1);
        assert_eq!(text.dirty_bytes, 16);
        assert_eq!(text.max_dirty_ratio_milli, 250);
        assert_eq!(text.upload_calls, 1);
        assert_eq!(text.upload_bytes, 16);
        assert_eq!(text.glyph_misses, 3);
        assert_eq!(text.atlas_evictions, 1);
        assert_eq!(snapshot.mailbox_overwrites, 1);
        assert_eq!(snapshot.revision_lag_max, 2);
        assert_eq!(snapshot.coalesced_updates, 2);
        assert_eq!(snapshot.command_ack_count, 1);
        assert!(metrics.profile_report().contains("gate_decision=deferred"));
        assert!(metrics.profile_report().contains("glyph_uploads=1"));
        assert!(metrics.profile_report().contains("coalesced_updates=2"));
    }
}
