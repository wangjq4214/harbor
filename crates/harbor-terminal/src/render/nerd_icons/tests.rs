use super::*;

#[test]
fn natural_icon_geometry_preserves_all_ink_bearings_and_uvs() {
    let surface = (256.0, 128.0);
    for (left, top, width, height) in [
        (-4, -3, 35, 40),
        (2, 3, 35, 20),
        (0, 0, 3, 4),
        (-35, -20, 35, 20),
        (30, 40, 35, 20),
    ] {
        let bounds = harbor_text::TileBounds {
            left,
            top,
            width,
            height,
        };
        for origin in [[10.0, 5.0], [3.25, 2.5], [250.0, 120.0]] {
            let uv = [0.1, 0.2, 0.7, 0.9];
            let vertices = natural_quad(bounds, uv, origin, surface, [1.0; 4]).unwrap();
            let x = (vertices[0].position[0] + 1.0) * surface.0 * 0.5;
            let y = (1.0 - vertices[0].position[1]) * surface.1 * 0.5;
            let ink_width = (vertices[2].position[0] - vertices[0].position[0]) * surface.0 * 0.5;
            let ink_height = (vertices[0].position[1] - vertices[2].position[1]) * surface.1 * 0.5;
            assert!((x - origin[0] - left as f32).abs() < 0.001);
            assert!((y - origin[1] - top as f32).abs() < 0.001);
            assert!((ink_width - width as f32).abs() < 0.001);
            assert!((ink_height - height as f32).abs() < 0.001);
            assert_eq!(vertices[0].tex_coords, [uv[0], uv[1]]);
            assert_eq!(vertices[2].tex_coords, [uv[2], uv[3]]);
            assert!(vertices.iter().all(|v| v.color == [1.0; 4]));
        }
    }
}

#[test]
fn empty_icon_artwork_emits_no_quad() {
    for (width, height) in [(0, 20), (35, 0), (0, 0)] {
        assert!(
            natural_quad(
                harbor_text::TileBounds {
                    left: -4,
                    top: -3,
                    width,
                    height
                },
                [0.0, 0.0, 1.0, 1.0],
                [10.0, 5.0],
                (256.0, 128.0),
                [1.0; 4],
            )
            .is_none()
        );
    }
}

#[test]
fn exact_collection_preserves_holes_and_excludes_other_pua() {
    assert_eq!(
        COLLECTION.iter().map(|(a, b)| b - a + 1).sum::<u32>(),
        10_386
    );
    for &(a, b) in COLLECTION {
        assert!(contains(char::from_u32(a).unwrap()));
        assert!(contains(char::from_u32(b).unwrap()));
    }
    for ch in [
        'A',
        '\u{e010}',
        '\u{e0c9}',
        '\u{ea89}',
        '\u{f0388}',
        '\u{f8ff}',
        '\u{100000}',
    ] {
        assert!(!contains(ch), "{ch:?}");
    }
    assert!(contains('\u{e5ff}'));
    assert!(contains('\u{f0001}'));
}
