//! Renderer-only Nerd Fonts membership and natural-size artwork presentation.

/// Exact scalar set (10,386 unique codepoints), compressed without filling holes.
/// Source: https://github.com/ryanoasis/nerd-fonts/blob/v3.4.0/glyphnames.json
/// SHA256: e2d10d23f5bff0bd6f0676e9b01d9789fcdc656de7b498a2955c27716ea4439c
/// Generated from the "code" values, sorted/deduplicated, merging only adjacent values.
/// This is collection membership, NOT a private-use Unicode block test.
const COLLECTION: &[(u32, u32)] = &[
    (0x23fb, 0x23fe),
    (0x2665, 0x2665),
    (0x26a1, 0x26a1),
    (0x2b58, 0x2b58),
    (0xe000, 0xe00a),
    (0xe0a0, 0xe0a3),
    (0xe0b0, 0xe0c8),
    (0xe0ca, 0xe0ca),
    (0xe0cc, 0xe0d2),
    (0xe0d4, 0xe0d4),
    (0xe0d6, 0xe0d7),
    (0xe200, 0xe2a9),
    (0xe300, 0xe3e3),
    (0xe5fa, 0xe6b8),
    (0xe700, 0xe8ef),
    (0xea60, 0xea88),
    (0xea8a, 0xea8c),
    (0xea8f, 0xeac7),
    (0xeac9, 0xeac9),
    (0xeacc, 0xeafa),
    (0xeafc, 0xeb09),
    (0xeb0b, 0xeb4e),
    (0xeb50, 0xec1e),
    (0xed00, 0xefce),
    (0xf000, 0xf381),
    (0xf400, 0xf533),
    (0xf0001, 0xf0387),
    (0xf0389, 0xf043c),
    (0xf043e, 0xf0508),
    (0xf050a, 0xf05fb),
    (0xf05fd, 0xf0764),
    (0xf0767, 0xf08cf),
    (0xf08d1, 0xf0b38),
    (0xf0b3a, 0xf0c9d),
    (0xf0ca0, 0xf1087),
    (0xf1089, 0xf108b),
    (0xf108d, 0xf1090),
    (0xf1092, 0xf13a5),
    (0xf13a7, 0xf189f),
    (0xf18a1, 0xf18ef),
    (0xf18f1, 0xf1af0),
];

pub(super) fn contains(ch: char) -> bool {
    let cp = ch as u32;
    let i = COLLECTION.partition_point(|&(_, end)| end < cp);
    COLLECTION.get(i).is_some_and(|&(start, _)| start <= cp)
}

/// Preserve the full native ink rectangle, including negative bearings and UVs.
/// Cell/row bounds and neighboring occupancy do not constrain icon artwork;
/// the host's overall viewport clipping still applies when this quad is drawn.
pub(super) fn natural_quad(
    bounds: harbor_text::TileBounds,
    uv: [f32; 4],
    origin: [f32; 2],
    surface: (f32, f32),
    color: [f32; 4],
) -> Option<[super::gpu::TexturedVertex; 6]> {
    if bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    let left = origin[0] + bounds.left as f32;
    let top = origin[1] + bounds.top as f32;
    Some(super::gpu::TexturedVertex::from_pixel_rect(
        left,
        top,
        left + bounds.width as f32,
        top + bounds.height as f32,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
        color,
        surface.0,
        surface.1,
    ))
}

#[cfg(test)]
mod tests;
