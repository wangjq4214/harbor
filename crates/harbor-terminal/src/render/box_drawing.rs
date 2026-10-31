//! Character-local, hard-edged terminal box drawing. No font or neighbor state.
use super::gpu::{self, ColoredVertex, TerminalGpuAccess};
use super::layout::RenderViewport;
use super::text::glyph_color_with_palette;
use crate::{Cell, CellAttrs, DirtyRange, TerminalSnapshot};
use harbor_config::Palette;
use std::sync::Arc;

// Eight clipped arc quads (at most 18 vertices each) and two rectangular arms.
// Independent of the underline slot layout.
pub(super) const BOX_VERTICES_PER_CELL: usize = 26 * 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Straight,
    Dash2,
    Dash3,
    Dash4,
    Round,
    Slash,
    Backslash,
    Cross,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    // left, right, up, down: absent=0, light=1, heavy=2, double=3.
    arms: [u8; 4],
    kind: Kind,
}
impl Shape {
    const fn new(arms: [u8; 4], kind: Kind) -> Self {
        Self { arms, kind }
    }
}

// Unicode Box Drawing repertoire, in scalar order. Explicit mixed-arm entries
// avoid a font-dependent fallback and make every endpoint independently auditable.
const SHAPES: [Shape; 128] = [
    Shape::new([1, 1, 0, 0], Kind::Straight), // U+2500 LIGHT HORIZONTAL
    Shape::new([2, 2, 0, 0], Kind::Straight), // U+2501 HEAVY HORIZONTAL
    Shape::new([0, 0, 1, 1], Kind::Straight), // U+2502 LIGHT VERTICAL
    Shape::new([0, 0, 2, 2], Kind::Straight), // U+2503 HEAVY VERTICAL
    Shape::new([1, 1, 0, 0], Kind::Dash3),    // U+2504 LIGHT TRIPLE DASH HORIZONTAL
    Shape::new([2, 2, 0, 0], Kind::Dash3),    // U+2505 HEAVY TRIPLE DASH HORIZONTAL
    Shape::new([0, 0, 1, 1], Kind::Dash3),    // U+2506 LIGHT TRIPLE DASH VERTICAL
    Shape::new([0, 0, 2, 2], Kind::Dash3),    // U+2507 HEAVY TRIPLE DASH VERTICAL
    Shape::new([1, 1, 0, 0], Kind::Dash4),    // U+2508 LIGHT QUADRUPLE DASH HORIZONTAL
    Shape::new([2, 2, 0, 0], Kind::Dash4),    // U+2509 HEAVY QUADRUPLE DASH HORIZONTAL
    Shape::new([0, 0, 1, 1], Kind::Dash4),    // U+250A LIGHT QUADRUPLE DASH VERTICAL
    Shape::new([0, 0, 2, 2], Kind::Dash4),    // U+250B HEAVY QUADRUPLE DASH VERTICAL
    Shape::new([0, 1, 0, 1], Kind::Straight), // U+250C LIGHT DOWN AND RIGHT
    Shape::new([0, 2, 0, 1], Kind::Straight), // U+250D DOWN LIGHT AND RIGHT HEAVY
    Shape::new([0, 1, 0, 2], Kind::Straight), // U+250E DOWN HEAVY AND RIGHT LIGHT
    Shape::new([0, 2, 0, 2], Kind::Straight), // U+250F HEAVY DOWN AND RIGHT
    Shape::new([1, 0, 0, 1], Kind::Straight), // U+2510 LIGHT DOWN AND LEFT
    Shape::new([2, 0, 0, 1], Kind::Straight), // U+2511 DOWN LIGHT AND LEFT HEAVY
    Shape::new([1, 0, 0, 2], Kind::Straight), // U+2512 DOWN HEAVY AND LEFT LIGHT
    Shape::new([2, 0, 0, 2], Kind::Straight), // U+2513 HEAVY DOWN AND LEFT
    Shape::new([0, 1, 1, 0], Kind::Straight), // U+2514 LIGHT UP AND RIGHT
    Shape::new([0, 2, 1, 0], Kind::Straight), // U+2515 UP LIGHT AND RIGHT HEAVY
    Shape::new([0, 1, 2, 0], Kind::Straight), // U+2516 UP HEAVY AND RIGHT LIGHT
    Shape::new([0, 2, 2, 0], Kind::Straight), // U+2517 HEAVY UP AND RIGHT
    Shape::new([1, 0, 1, 0], Kind::Straight), // U+2518 LIGHT UP AND LEFT
    Shape::new([2, 0, 1, 0], Kind::Straight), // U+2519 UP LIGHT AND LEFT HEAVY
    Shape::new([1, 0, 2, 0], Kind::Straight), // U+251A UP HEAVY AND LEFT LIGHT
    Shape::new([2, 0, 2, 0], Kind::Straight), // U+251B HEAVY UP AND LEFT
    Shape::new([0, 1, 1, 1], Kind::Straight), // U+251C LIGHT VERTICAL AND RIGHT
    Shape::new([0, 2, 1, 1], Kind::Straight), // U+251D VERTICAL LIGHT AND RIGHT HEAVY
    Shape::new([0, 1, 2, 1], Kind::Straight), // U+251E UP HEAVY AND RIGHT DOWN LIGHT
    Shape::new([0, 1, 1, 2], Kind::Straight), // U+251F DOWN HEAVY AND RIGHT UP LIGHT
    Shape::new([0, 1, 2, 2], Kind::Straight), // U+2520 VERTICAL HEAVY AND RIGHT LIGHT
    Shape::new([0, 2, 2, 1], Kind::Straight), // U+2521 DOWN LIGHT AND RIGHT UP HEAVY
    Shape::new([0, 2, 1, 2], Kind::Straight), // U+2522 UP LIGHT AND RIGHT DOWN HEAVY
    Shape::new([0, 2, 2, 2], Kind::Straight), // U+2523 HEAVY VERTICAL AND RIGHT
    Shape::new([1, 0, 1, 1], Kind::Straight), // U+2524 LIGHT VERTICAL AND LEFT
    Shape::new([2, 0, 1, 1], Kind::Straight), // U+2525 VERTICAL LIGHT AND LEFT HEAVY
    Shape::new([1, 0, 2, 1], Kind::Straight), // U+2526 UP HEAVY AND LEFT DOWN LIGHT
    Shape::new([1, 0, 1, 2], Kind::Straight), // U+2527 DOWN HEAVY AND LEFT UP LIGHT
    Shape::new([1, 0, 2, 2], Kind::Straight), // U+2528 VERTICAL HEAVY AND LEFT LIGHT
    Shape::new([2, 0, 2, 1], Kind::Straight), // U+2529 DOWN LIGHT AND LEFT UP HEAVY
    Shape::new([2, 0, 1, 2], Kind::Straight), // U+252A UP LIGHT AND LEFT DOWN HEAVY
    Shape::new([2, 0, 2, 2], Kind::Straight), // U+252B HEAVY VERTICAL AND LEFT
    Shape::new([1, 1, 0, 1], Kind::Straight), // U+252C LIGHT DOWN AND HORIZONTAL
    Shape::new([2, 1, 0, 1], Kind::Straight), // U+252D LEFT HEAVY AND RIGHT DOWN LIGHT
    Shape::new([1, 2, 0, 1], Kind::Straight), // U+252E RIGHT HEAVY AND LEFT DOWN LIGHT
    Shape::new([2, 2, 0, 1], Kind::Straight), // U+252F DOWN LIGHT AND HORIZONTAL HEAVY
    Shape::new([1, 1, 0, 2], Kind::Straight), // U+2530 DOWN HEAVY AND HORIZONTAL LIGHT
    Shape::new([2, 1, 0, 2], Kind::Straight), // U+2531 RIGHT LIGHT AND LEFT DOWN HEAVY
    Shape::new([1, 2, 0, 2], Kind::Straight), // U+2532 LEFT LIGHT AND RIGHT DOWN HEAVY
    Shape::new([2, 2, 0, 2], Kind::Straight), // U+2533 HEAVY DOWN AND HORIZONTAL
    Shape::new([1, 1, 1, 0], Kind::Straight), // U+2534 LIGHT UP AND HORIZONTAL
    Shape::new([2, 1, 1, 0], Kind::Straight), // U+2535 LEFT HEAVY AND RIGHT UP LIGHT
    Shape::new([1, 2, 1, 0], Kind::Straight), // U+2536 RIGHT HEAVY AND LEFT UP LIGHT
    Shape::new([2, 2, 1, 0], Kind::Straight), // U+2537 UP LIGHT AND HORIZONTAL HEAVY
    Shape::new([1, 1, 2, 0], Kind::Straight), // U+2538 UP HEAVY AND HORIZONTAL LIGHT
    Shape::new([2, 1, 2, 0], Kind::Straight), // U+2539 RIGHT LIGHT AND LEFT UP HEAVY
    Shape::new([1, 2, 2, 0], Kind::Straight), // U+253A LEFT LIGHT AND RIGHT UP HEAVY
    Shape::new([2, 2, 2, 0], Kind::Straight), // U+253B HEAVY UP AND HORIZONTAL
    Shape::new([1, 1, 1, 1], Kind::Straight), // U+253C LIGHT VERTICAL AND HORIZONTAL
    Shape::new([2, 1, 1, 1], Kind::Straight), // U+253D LEFT HEAVY AND RIGHT VERTICAL LIGHT
    Shape::new([1, 2, 1, 1], Kind::Straight), // U+253E RIGHT HEAVY AND LEFT VERTICAL LIGHT
    Shape::new([2, 2, 1, 1], Kind::Straight), // U+253F VERTICAL LIGHT AND HORIZONTAL HEAVY
    Shape::new([1, 1, 2, 1], Kind::Straight), // U+2540 UP HEAVY AND DOWN HORIZONTAL LIGHT
    Shape::new([1, 1, 1, 2], Kind::Straight), // U+2541 DOWN HEAVY AND UP HORIZONTAL LIGHT
    Shape::new([1, 1, 2, 2], Kind::Straight), // U+2542 VERTICAL HEAVY AND HORIZONTAL LIGHT
    Shape::new([2, 1, 2, 1], Kind::Straight), // U+2543 LEFT UP HEAVY AND RIGHT DOWN LIGHT
    Shape::new([1, 2, 2, 1], Kind::Straight), // U+2544 RIGHT UP HEAVY AND LEFT DOWN LIGHT
    Shape::new([2, 1, 1, 2], Kind::Straight), // U+2545 LEFT DOWN HEAVY AND RIGHT UP LIGHT
    Shape::new([1, 2, 1, 2], Kind::Straight), // U+2546 RIGHT DOWN HEAVY AND LEFT UP LIGHT
    Shape::new([2, 2, 2, 1], Kind::Straight), // U+2547 DOWN LIGHT AND UP HORIZONTAL HEAVY
    Shape::new([2, 2, 1, 2], Kind::Straight), // U+2548 UP LIGHT AND DOWN HORIZONTAL HEAVY
    Shape::new([2, 1, 2, 2], Kind::Straight), // U+2549 RIGHT LIGHT AND LEFT VERTICAL HEAVY
    Shape::new([1, 2, 2, 2], Kind::Straight), // U+254A LEFT LIGHT AND RIGHT VERTICAL HEAVY
    Shape::new([2, 2, 2, 2], Kind::Straight), // U+254B HEAVY VERTICAL AND HORIZONTAL
    Shape::new([1, 1, 0, 0], Kind::Dash2),    // U+254C LIGHT DOUBLE DASH HORIZONTAL
    Shape::new([2, 2, 0, 0], Kind::Dash2),    // U+254D HEAVY DOUBLE DASH HORIZONTAL
    Shape::new([0, 0, 1, 1], Kind::Dash2),    // U+254E LIGHT DOUBLE DASH VERTICAL
    Shape::new([0, 0, 2, 2], Kind::Dash2),    // U+254F HEAVY DOUBLE DASH VERTICAL
    Shape::new([3, 3, 0, 0], Kind::Straight), // U+2550 DOUBLE HORIZONTAL
    Shape::new([0, 0, 3, 3], Kind::Straight), // U+2551 DOUBLE VERTICAL
    Shape::new([0, 3, 0, 1], Kind::Straight), // U+2552 DOWN SINGLE AND RIGHT DOUBLE
    Shape::new([0, 1, 0, 3], Kind::Straight), // U+2553 DOWN DOUBLE AND RIGHT SINGLE
    Shape::new([0, 3, 0, 3], Kind::Straight), // U+2554 DOUBLE DOWN AND RIGHT
    Shape::new([3, 0, 0, 1], Kind::Straight), // U+2555 DOWN SINGLE AND LEFT DOUBLE
    Shape::new([1, 0, 0, 3], Kind::Straight), // U+2556 DOWN DOUBLE AND LEFT SINGLE
    Shape::new([3, 0, 0, 3], Kind::Straight), // U+2557 DOUBLE DOWN AND LEFT
    Shape::new([0, 3, 1, 0], Kind::Straight), // U+2558 UP SINGLE AND RIGHT DOUBLE
    Shape::new([0, 1, 3, 0], Kind::Straight), // U+2559 UP DOUBLE AND RIGHT SINGLE
    Shape::new([0, 3, 3, 0], Kind::Straight), // U+255A DOUBLE UP AND RIGHT
    Shape::new([3, 0, 1, 0], Kind::Straight), // U+255B UP SINGLE AND LEFT DOUBLE
    Shape::new([1, 0, 3, 0], Kind::Straight), // U+255C UP DOUBLE AND LEFT SINGLE
    Shape::new([3, 0, 3, 0], Kind::Straight), // U+255D DOUBLE UP AND LEFT
    Shape::new([0, 3, 1, 1], Kind::Straight), // U+255E VERTICAL SINGLE AND RIGHT DOUBLE
    Shape::new([0, 1, 3, 3], Kind::Straight), // U+255F VERTICAL DOUBLE AND RIGHT SINGLE
    Shape::new([0, 3, 3, 3], Kind::Straight), // U+2560 DOUBLE VERTICAL AND RIGHT
    Shape::new([3, 0, 1, 1], Kind::Straight), // U+2561 VERTICAL SINGLE AND LEFT DOUBLE
    Shape::new([1, 0, 3, 3], Kind::Straight), // U+2562 VERTICAL DOUBLE AND LEFT SINGLE
    Shape::new([3, 0, 3, 3], Kind::Straight), // U+2563 DOUBLE VERTICAL AND LEFT
    Shape::new([3, 3, 0, 1], Kind::Straight), // U+2564 DOWN SINGLE AND HORIZONTAL DOUBLE
    Shape::new([1, 1, 0, 3], Kind::Straight), // U+2565 DOWN DOUBLE AND HORIZONTAL SINGLE
    Shape::new([3, 3, 0, 3], Kind::Straight), // U+2566 DOUBLE DOWN AND HORIZONTAL
    Shape::new([3, 3, 1, 0], Kind::Straight), // U+2567 UP SINGLE AND HORIZONTAL DOUBLE
    Shape::new([1, 1, 3, 0], Kind::Straight), // U+2568 UP DOUBLE AND HORIZONTAL SINGLE
    Shape::new([3, 3, 3, 0], Kind::Straight), // U+2569 DOUBLE UP AND HORIZONTAL
    Shape::new([3, 3, 1, 1], Kind::Straight), // U+256A VERTICAL SINGLE AND HORIZONTAL DOUBLE
    Shape::new([1, 1, 3, 3], Kind::Straight), // U+256B VERTICAL DOUBLE AND HORIZONTAL SINGLE
    Shape::new([3, 3, 3, 3], Kind::Straight), // U+256C DOUBLE VERTICAL AND HORIZONTAL
    Shape::new([0, 1, 0, 1], Kind::Round),    // U+256D LIGHT ARC DOWN AND RIGHT
    Shape::new([1, 0, 0, 1], Kind::Round),    // U+256E LIGHT ARC DOWN AND LEFT
    Shape::new([1, 0, 1, 0], Kind::Round),    // U+256F LIGHT ARC UP AND LEFT
    Shape::new([0, 1, 1, 0], Kind::Round),    // U+2570 LIGHT ARC UP AND RIGHT
    Shape::new([0, 0, 0, 0], Kind::Slash),    // U+2571 LIGHT DIAGONAL UPPER RIGHT TO LOWER LEFT
    Shape::new([0, 0, 0, 0], Kind::Backslash), // U+2572 LIGHT DIAGONAL UPPER LEFT TO LOWER RIGHT
    Shape::new([0, 0, 0, 0], Kind::Cross),    // U+2573 LIGHT DIAGONAL CROSS
    Shape::new([1, 0, 0, 0], Kind::Straight), // U+2574 LIGHT LEFT
    Shape::new([0, 0, 1, 0], Kind::Straight), // U+2575 LIGHT UP
    Shape::new([0, 1, 0, 0], Kind::Straight), // U+2576 LIGHT RIGHT
    Shape::new([0, 0, 0, 1], Kind::Straight), // U+2577 LIGHT DOWN
    Shape::new([2, 0, 0, 0], Kind::Straight), // U+2578 HEAVY LEFT
    Shape::new([0, 0, 2, 0], Kind::Straight), // U+2579 HEAVY UP
    Shape::new([0, 2, 0, 0], Kind::Straight), // U+257A HEAVY RIGHT
    Shape::new([0, 0, 0, 2], Kind::Straight), // U+257B HEAVY DOWN
    Shape::new([1, 2, 0, 0], Kind::Straight), // U+257C LIGHT LEFT AND HEAVY RIGHT
    Shape::new([0, 0, 1, 2], Kind::Straight), // U+257D LIGHT UP AND HEAVY DOWN
    Shape::new([2, 1, 0, 0], Kind::Straight), // U+257E HEAVY LEFT AND LIGHT RIGHT
    Shape::new([0, 0, 2, 1], Kind::Straight), // U+257F HEAVY UP AND LIGHT DOWN
];

pub(super) fn is_box_drawing(ch: char) -> bool {
    ('\u{2500}'..='\u{257f}').contains(&ch)
}

fn shape(ch: char) -> Option<Shape> {
    is_box_drawing(ch).then(|| SHAPES[ch as usize - 0x2500])
}

fn drawable(view: &RenderViewport) -> bool {
    view.is_drawable()
}

type Point = [f64; 2];

struct Mesh {
    vertices: [ColoredVertex; BOX_VERTICES_PER_CELL],
    used: usize,
    clip: [f64; 4],
    surface: [f64; 2],
    color: [f32; 4],
}

impl Mesh {
    // Convex quad clipped to cell, allocation and surface. Fixed arrays/loop
    // counts bound work even when the supplied physical metrics are enormous.
    fn quad(&mut self, points: [Point; 4]) {
        let mut polygon = [[0.0; 2]; 12];
        polygon[..4].copy_from_slice(&points);
        let mut len = 4;
        for (axis, boundary, greater) in [
            (0, self.clip[0], true),
            (1, self.clip[1], true),
            (0, self.clip[2], false),
            (1, self.clip[3], false),
        ] {
            if len == 0 {
                return;
            }
            let mut next = [[0.0; 2]; 12];
            let mut count = 0;
            let mut prev = polygon[len - 1];
            let inside = |p: Point| {
                if greater {
                    p[axis] >= boundary
                } else {
                    p[axis] <= boundary
                }
            };
            for &current in &polygon[..len] {
                if inside(prev) != inside(current) {
                    let ratio = (boundary - prev[axis]) / (current[axis] - prev[axis]);
                    let mut p = [
                        prev[0] + ratio * (current[0] - prev[0]),
                        prev[1] + ratio * (current[1] - prev[1]),
                    ];
                    p[axis] = boundary;
                    next[count] = p;
                    count += 1;
                }
                if inside(current) {
                    next[count] = current;
                    count += 1;
                }
                prev = current;
            }
            polygon = next;
            len = count;
        }
        if len < 3 {
            return;
        }
        for i in 1..len - 1 {
            for p in [polygon[0], polygon[i], polygon[i + 1]] {
                self.vertices[self.used] = ColoredVertex {
                    position: [
                        (p[0] / self.surface[0] * 2.0 - 1.0) as f32,
                        (1.0 - p[1] / self.surface[1] * 2.0) as f32,
                    ],
                    color: self.color,
                };
                self.used += 1;
            }
        }
    }

    fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if x1 > x0 && y1 > y0 {
            self.quad([[x0, y0], [x0, y1], [x1, y1], [x1, y0]]);
        }
    }

    fn diagonal(&mut self, start: Point, end: Point, thickness: f64) {
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let length = dx.hypot(dy);
        if length == 0.0 {
            return;
        }
        let x = -dy / length * thickness / 2.0;
        let y = dx / length * thickness / 2.0;
        self.quad([
            [start[0] + x, start[1] + y],
            [start[0] - x, start[1] - y],
            [end[0] - x, end[1] - y],
            [end[0] + x, end[1] + y],
        ]);
    }
}

fn band(center: f64, thickness: f64) -> (f64, f64) {
    let start = (center - thickness / 2.0).round();
    (start, start + thickness)
}

pub(super) fn cell_vertices(
    cell: &Cell,
    view: &RenderViewport,
    palette: &Palette,
    row: usize,
    col: usize,
) -> [ColoredVertex; BOX_VERTICES_PER_CELL] {
    let empty = [ColoredVertex::default(); BOX_VERTICES_PER_CELL];
    if !drawable(view) || cell.wide_continuation || cell.attrs.contains(CellAttrs::CONCEAL) {
        return empty;
    }
    let Some(shape) = shape(cell.ch) else {
        return empty;
    };
    // Both sides of a shared edge use this identical indexed expression, NOT
    // a rounded cell origin plus a separately rounded width. f64 intermediates
    // also prevent finite f32 dimensions/origins overflowing during projection.
    let edge = |origin: f32, size: f32, index: usize| {
        (f64::from(origin) + f64::from(view.padding) + index as f64 * f64::from(size)).round()
    };
    let l = edge(view.allocation_origin.0, view.cell_width, col);
    let r = edge(
        view.allocation_origin.0,
        view.cell_width,
        col.saturating_add(1),
    );
    let u = edge(view.allocation_origin.1, view.line_height, row);
    let b = edge(
        view.allocation_origin.1,
        view.line_height,
        row.saturating_add(1),
    );
    if r <= l || b <= u {
        return empty;
    }
    let clip = [
        l.max(f64::from(view.allocation_origin.0)).max(0.0),
        u.max(f64::from(view.allocation_origin.1)).max(0.0),
        r.min(f64::from(view.allocation_origin.0) + f64::from(view.allocation_size.0))
            .min(f64::from(view.surface_size.0)),
        b.min(f64::from(view.allocation_origin.1) + f64::from(view.allocation_size.1))
            .min(f64::from(view.surface_size.1)),
    ];
    if clip[2] <= clip[0] || clip[3] <= clip[1] {
        return empty;
    }
    let mut mesh = Mesh {
        vertices: empty,
        used: 0,
        clip,
        surface: [
            f64::from(view.surface_size.0),
            f64::from(view.surface_size.1),
        ],
        color: glyph_color_with_palette(palette, cell.fg, cell.bg, cell.attrs),
    };
    let cx = (l + r) / 2.0;
    let cy = (u + b) / 2.0;
    let t = (f64::from(view.cell_width.min(view.line_height)) / 8.0)
        .round()
        .max(1.0);
    let [left, right, up, down] = shape.arms;
    match shape.kind {
        Kind::Slash | Kind::Backslash | Kind::Cross => {
            if shape.kind != Kind::Backslash {
                mesh.diagonal([l, b], [r, u], t);
            }
            if shape.kind != Kind::Slash {
                mesh.diagonal([l, u], [r, b], t);
            }
        }
        Kind::Round => {
            let sx = if right != 0 { 1.0 } else { -1.0 };
            let sy = if down != 0 { 1.0 } else { -1.0 };
            let radius = ((r - l).min(b - u) / 4.0).max(t);
            let center = [cx + sx * radius, cy + sy * radius];
            let point = |angle: f64, radius: f64| {
                [
                    center[0] - sx * radius * angle.sin(),
                    center[1] - sy * radius * angle.cos(),
                ]
            };
            for i in 0..8 {
                let a = i as f64 * std::f64::consts::FRAC_PI_2 / 8.0;
                let z = (i + 1) as f64 * std::f64::consts::FRAC_PI_2 / 8.0;
                mesh.quad([
                    point(a, radius + t / 2.0),
                    point(z, radius + t / 2.0),
                    point(z, (radius - t / 2.0).max(0.0)),
                    point(a, (radius - t / 2.0).max(0.0)),
                ]);
            }
            // Arc endpoints share the same pixel-aligned bands as straight arms.
            let (y0, y1) = band(cy, t);
            let (x0, x1) = band(cx, t);
            if right != 0 {
                mesh.rect(center[0] - t / 2.0, y0, r, y1);
            } else {
                mesh.rect(l, y0, center[0] + t / 2.0, y1);
            }
            if down != 0 {
                mesh.rect(x0, center[1] - t / 2.0, x1, b);
            } else {
                mesh.rect(x0, u, x1, center[1] + t / 2.0);
            }
        }
        Kind::Dash2 | Kind::Dash3 | Kind::Dash4 => {
            let count = match shape.kind {
                Kind::Dash2 => 2,
                Kind::Dash3 => 3,
                _ => 4,
            };
            let horizontal = left != 0;
            let (start, end) = if horizontal { (l, r) } else { (u, b) };
            let thickness = t * f64::from(if horizontal { left } else { up });
            let (lo, hi) = band(if horizontal { cy } else { cx }, thickness);
            for i in 0..count {
                let a = start + (end - start) * (i as f64 + 0.15) / count as f64;
                let z = start + (end - start) * (i as f64 + 0.85) / count as f64;
                if horizontal {
                    mesh.rect(a, lo, z, hi);
                } else {
                    mesh.rect(lo, a, hi, z);
                }
            }
        }
        Kind::Straight => {
            // Double arms turn into matching perpendicular lanes. At a tee/cross
            // they stop at the inner lanes, leaving the intended central gap.
            // A single perpendicular bar instead joins both double lanes.
            let d = t;
            for (dir, style) in shape.arms.into_iter().enumerate() {
                if style == 0 {
                    continue;
                }
                let horizontal = dir < 2;
                let positive = dir == 1 || dir == 3;
                let (center, cross, low, high, p0, p1) = if horizontal {
                    (cx, cy, l, r, up, down)
                } else {
                    (cy, cx, u, b, left, right)
                };
                let lanes: &[f64] = if style == 3 { &[-1.0, 1.0] } else { &[0.0] };
                for &lane in lanes {
                    let thickness = if style == 3 { t } else { t * f64::from(style) };
                    let (lo, hi) = band(cross + lane * d, thickness);
                    let perpendicular = if lane < 0.0 { p0 } else { p1 };
                    let reach = if style == 3 {
                        if perpendicular == 3 {
                            if positive { d } else { -d }
                        } else if perpendicular == 0 {
                            if positive { -d } else { d }
                        } else {
                            0.0
                        }
                    } else if p0 == 3 || p1 == 3 {
                        if positive { -d } else { d }
                    } else {
                        0.0
                    };
                    // For an isolated half-line, stop at the center exactly.
                    let cap = if p0 != 0 || p1 != 0 { t / 2.0 } else { 0.0 };
                    let (a, z) = if positive {
                        (center + reach - cap, high)
                    } else {
                        (low, center + reach + cap)
                    };
                    if horizontal {
                        mesh.rect(a, lo, z, hi);
                    } else {
                        mesh.rect(lo, a, hi, z);
                    }
                }
            }
        }
    }
    mesh.vertices
}

pub(super) fn slot_count(snap: &TerminalSnapshot, max_buffer_size: u64) -> Option<usize> {
    if snap.rows == 0 || snap.cols == 0 {
        return None;
    }
    let cells = snap.rows.checked_mul(snap.cols)?;
    if snap.cells.len() < cells {
        return None;
    }
    let vertices = cells.checked_mul(BOX_VERTICES_PER_CELL)?;
    let bytes = vertices.checked_mul(std::mem::size_of::<ColoredVertex>())?;
    (vertices <= u32::MAX as usize && bytes as u64 <= max_buffer_size).then_some(vertices)
}

pub(super) struct BoxDrawing {
    pipeline: Arc<wgpu::RenderPipeline>,
    geometry: gpu::ColoredRows,
    grid: (usize, usize),
    viewport: Option<RenderViewport>,
    palette: Palette,
    dirty: bool,
}

impl BoxDrawing {
    pub(super) fn new(
        _gpu: TerminalGpuAccess<'_>,
        pipeline: Arc<wgpu::RenderPipeline>,
        palette: Palette,
    ) -> Self {
        Self {
            pipeline,
            geometry: gpu::ColoredRows::default(),
            grid: (0, 0),
            viewport: None,
            palette,
            dirty: true,
        }
    }

    pub(super) fn invalidate_projection(&mut self) {
        self.dirty = true;
    }

    pub(super) fn set_palette(&mut self, palette: Palette) {
        if self.palette != palette {
            self.palette = palette;
            self.dirty = true;
        }
    }

    pub(super) fn prepare_with_dirty(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        snap: &TerminalSnapshot,
        ranges: &[DirtyRange],
        view: &RenderViewport,
    ) {
        if !drawable(view) || slot_count(snap, gpu.device().limits().max_buffer_size).is_none() {
            self.viewport = None;
            self.dirty = true;
            return;
        }
        let full = self.dirty
            || self.grid != (snap.rows, snap.cols)
            || self.viewport != Some(*view)
            || ranges
                .iter()
                .any(|r| r.row >= snap.rows || r.start_col > r.end_col || r.end_col > snap.cols);
        self.geometry
            .prepare(gpu, snap.rows, ranges, full, |row, vertices| {
                for col in 0..snap.cols {
                    let cell = snap.cell(row, col);
                    if is_box_drawing(cell.ch)
                        && !cell.wide_continuation
                        && !cell.attrs.contains(CellAttrs::CONCEAL)
                    {
                        gpu::ColoredRows::append(
                            vertices,
                            &cell_vertices(cell, view, &self.palette, row, col),
                        );
                    }
                }
            });
        self.grid = (snap.rows, snap.cols);
        self.viewport = Some(*view);
        self.dirty = false;
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.viewport.is_some() {
            self.geometry.draw(pass, &self.pipeline);
        }
    }
}

#[cfg(test)]
#[path = "box_drawing_tests.rs"]
mod tests;
