//! Renderer-independent scrollbar hit testing shared by pointer input and the GPU layer.
use crate::layout::RenderViewport;
use crate::model::TerminalSnapshot;
use harbor_config::{SCROLLBAR_MARGIN, SCROLLBAR_MIN_THUMB_HEIGHT, SCROLLBAR_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollbarHit {
    None,
    Thumb { grab_offset: f32 },
    TrackBefore,
    TrackAfter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeometry {
    track_top: f32,
    thumb_height: f32,
    track_bottom: f32,
    thumb_rect: [f32; 4],
}

impl ScrollbarGeometry {
    fn compute(snap: &TerminalSnapshot, viewport: &RenderViewport) -> Option<Self> {
        if snap.is_alt || snap.scroll_count == 0 {
            return None;
        }
        let (origin_x, origin_y) = viewport.allocation_origin;
        let alloc_w = viewport.allocation_size.0 as f32;
        let alloc_h = viewport.allocation_size.1 as f32;
        let track_top = origin_y + viewport.padding;
        let track_bottom = origin_y + alloc_h - viewport.padding;
        let track_height = track_bottom - track_top;
        if track_height <= 0.0 {
            return None;
        }
        let total_rows = snap.rows + snap.scroll_count;
        let thumb_height = ((snap.rows as f32 / total_rows as f32) * track_height)
            .max(SCROLLBAR_MIN_THUMB_HEIGHT)
            .min(track_height);
        let scroll_fraction = 1.0 - (snap.view_offset as f32 / snap.scroll_count as f32);
        let thumb_top = track_top + scroll_fraction * (track_height - thumb_height);
        let right = origin_x + alloc_w - SCROLLBAR_MARGIN;
        Some(Self {
            track_top,
            thumb_height,
            track_bottom,
            thumb_rect: [
                right - SCROLLBAR_WIDTH,
                thumb_top,
                right,
                thumb_top + thumb_height,
            ],
        })
    }
}

pub fn hit_test(
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    point: (f32, f32),
) -> ScrollbarHit {
    let Some(geometry) = ScrollbarGeometry::compute(snap, viewport) else {
        return ScrollbarHit::None;
    };
    let [left, top, right, bottom] = geometry.thumb_rect;
    if point.0 < left
        || point.0 > right
        || point.1 < geometry.track_top
        || point.1 > geometry.track_bottom
    {
        return ScrollbarHit::None;
    }
    if point.1 < top {
        ScrollbarHit::TrackBefore
    } else if point.1 > bottom {
        ScrollbarHit::TrackAfter
    } else {
        ScrollbarHit::Thumb {
            grab_offset: (point.1 - top).clamp(0.0, bottom - top),
        }
    }
}

pub fn offset_for_thumb(
    snap: &TerminalSnapshot,
    viewport: &RenderViewport,
    pointer_y: f32,
    grab_offset: f32,
) -> Option<usize> {
    let geometry = ScrollbarGeometry::compute(snap, viewport)?;
    let movable = (geometry.track_bottom - geometry.track_top - geometry.thumb_height).max(0.0);
    if movable == 0.0 {
        return Some(0);
    }
    let thumb_top =
        (pointer_y - grab_offset).clamp(geometry.track_top, geometry.track_top + movable);
    let fraction_from_bottom = 1.0 - ((thumb_top - geometry.track_top) / movable);
    Some((fraction_from_bottom * snap.scroll_count as f32).round() as usize)
}

#[cfg(feature = "renderer")]
pub fn compute_thumb_rect(snap: &TerminalSnapshot, viewport: &RenderViewport) -> Option<[f32; 4]> {
    ScrollbarGeometry::compute(snap, viewport).map(|geometry| geometry.thumb_rect)
}
