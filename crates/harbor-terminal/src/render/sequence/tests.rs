#[path = "tests/gpu.rs"]
mod gpu;

use super::*;
#[test]
fn fitted_native_bearings_and_oversized_ink_stay_uniform_and_inside_cell() {
    for (left, top, w, h) in [(-20, -50, 100, 80), (8, 9, 10, 12), (500, 500, 20, 20)] {
        let q = fitted_quad(
            TileBounds {
                left,
                top,
                width: w,
                height: h,
            },
            [0.1, 0.2, 0.5, 0.8],
            [20.0, 30.0, 18.0, 19.0],
            (100.0, 100.0),
            [1.0; 4],
        )
        .unwrap();
        let xy = |v: TexturedVertex| ((v.position[0] + 1.0) * 50.0, (1.0 - v.position[1]) * 50.0);
        let (l, t) = xy(q[0]);
        let (r, b) = xy(q[2]);
        assert!(l >= 19.999 && t >= 29.999 && r <= 38.001 && b <= 49.001);
        assert!(((r - l) / (b - t) - w as f32 / h as f32).abs() < 0.001);
    }
}
#[test]
fn classification_uses_complete_source_not_wide_cells() {
    let mut c = Cell {
        ch: '中',
        width: 2,
        ..Cell::default()
    };
    assert!(!emoji_candidate(&c));
    c.ch = '♥';
    assert!(!emoji_candidate(&c));
    c.suffix = "\u{fe0f}".into();
    assert!(emoji_candidate(&c));
    c.ch = '👩';
    c.suffix = "\u{200d}💻".into();
    assert!(emoji_candidate(&c));
    c.wide_continuation = true;
    assert!(!emoji_candidate(&c));
}
