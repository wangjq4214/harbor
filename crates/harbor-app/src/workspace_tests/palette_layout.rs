use super::palette_support::*;
use super::*;

#[test]
fn palette_row_height_is_uniform_across_navigation_and_filtered_result_counts() {
    for (width, height, scale, line_height) in [
        (1000, 900, 1.0, 20.0),
        (1500, 1350, 1.5, 20.0),
        (320, 1200, 1.0, 28.0),
    ] {
        for multiple_bindings in [false, true] {
            let (_, controller, mut runtime, bytes) = fixture();
            runtime.set_viewport(Viewport::new(width, height, scale));
            runtime.set_text_metrics(harbor_widget::text::TextMetrics {
                line_height,
                ..*runtime.text_metrics()
            });
            if multiple_bindings {
                let mut raw = harbor_config::RawKeybindings::default();
                raw.entries.insert(
                    "app.new-tab".into(),
                    ["ctrl", "alt", "shift"]
                        .into_iter()
                        .map(|modifier| harbor_config::RawKeybinding {
                            modifiers: vec![modifier.into()],
                            key: "n".into(),
                        })
                        .collect(),
                );
                raw.entries.insert("terminal.copy".into(), vec![]);
                controller.publish_keybindings(validate_keybindings(&raw).unwrap());
            }
            runtime.update(Instant::now());
            open(&controller, &mut runtime);
            let (full_panel, first_row) = palette_geometry(&runtime);
            let row_height = first_row.size().height;
            assert_palette_title_centered(&runtime, first_row, "Reload configuration", 1);
            let count = crate::command::command_registry()
                .iter()
                .filter(|entry| entry.palette_visible)
                .count();
            for index in 1..=count {
                tap(&mut runtime, Key::ArrowDown);
                runtime.update(Instant::now());
                let (panel, row) = palette_geometry(&runtime);
                assert_eq!(
                    panel, full_panel,
                    "the results viewport must not resize during navigation"
                );
                assert_eq!(
                    row.size().height,
                    row_height,
                    "all command rows have one height"
                );
                assert_eq!(
                    row.min.y,
                    first_row.min.y + (index % count).min(4) as f32 * (row_height + 4.0),
                    "selection advances in equal row steps, then keeps five rows visible",
                );
            }
            for (query, visible, title, lines) in [
                ("terminal.search", 4, "Search current session", 1),
                ("terminal.page-", 2, "Page up", 1),
                ("app.new-tab", 1, "New tab", 1),
                ("terminal.search-close", 1, "Close search", 1),
                ("terminal.copy", 1, "Copy", 1),
                ("", 5, "Reload configuration", 1),
            ] {
                replace(&mut runtime, query);
                let (panel, row) = palette_geometry(&runtime);
                assert_eq!(row.size().height, row_height, "query: {query}");
                assert_palette_title_centered(&runtime, row, title, lines);
                assert_eq!(
                    panel.size().height,
                    full_panel.size().height - (5 - visible) as f32 * (row_height + 4.0),
                    "fewer matches use their actual row count without empty slots: {query}",
                );
            }
            assert!(bytes.lock().unwrap().is_empty());
        }
    }
}

#[test]
fn palette_rows_reserve_a_gutter_with_right_aligned_keycaps_and_translucent_selection() {
    for (width, height, scale) in [(1000, 700, 1.0), (1500, 1000, 1.5), (320, 700, 1.0)] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let delta = runtime.pending_delta().unwrap();
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        let panel = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::BackdropMaterial { rect, .. } => Some(rect),
                _ => None,
            })
            .unwrap();
        let selection = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::RoundedQuad {
                    rect,
                    color,
                    corner_radii,
                } if corner_radii == [12.0; 4] && color.a > 0.0 => {
                    assert!(color.a <= 0.15, "selection must reveal the glass below it");
                    Some(rect)
                }
                _ => None,
            })
            .expect("selected command has a translucent highlight");
        assert!(
            items.iter().any(|item| matches!(item.primitive,
            Primitive::BackdropMaterial { rect, corner_radii, .. }
                if rect == selection && corner_radii == [12.0; 4])),
            "selection uses real rounded liquid glass, not only a translucent fill"
        );
        assert_eq!(selection.min.x, panel.min.x + 8.0);
        assert_eq!(selection.max.x, panel.max.x - 8.0 - 14.0);

        let caps: Vec<_> = items
            .iter()
            .filter(|item| {
                let Primitive::RoundedBorder { rect, .. } = item.primitive else {
                    return false;
                };
                item.clips.iter().all(|clip| {
                    let clip = clip.rect();
                    rect.min.y >= clip.min.y && rect.max.y <= clip.max.y
                })
            })
            .filter_map(|item| match item.primitive {
                Primitive::RoundedBorder {
                    rect,
                    width: 1.0,
                    corner_radii,
                    ..
                } if corner_radii == [4.0; 4] => Some(rect),
                _ => None,
            })
            .collect();
        assert_eq!(
            caps.len(),
            12,
            "each key in the five default shortcuts has a frame"
        );
        let mut rows: Vec<f32> = caps.iter().map(|rect| rect.min.y).collect();
        rows.sort_by(f32::total_cmp);
        rows.dedup();
        assert_eq!(rows.len(), 5);
        for y in rows {
            let right = caps
                .iter()
                .filter(|rect| rect.min.y == y)
                .map(|rect| rect.max.x)
                .fold(0.0, f32::max);
            assert_eq!(
                right,
                selection.max.x - 8.0,
                "shortcut groups share the right inset"
            );
        }
        for cap in &caps {
            assert!(cap.min.x >= selection.min.x + 8.0);
            let text = items
                .iter()
                .find_map(|item| match &item.primitive {
                    Primitive::Text { origin, text, .. }
                        if *origin == Point::new(cap.min.x + 2.0, cap.min.y + 1.0) =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("keycap text has only a small inset");
            let metrics = runtime.text_metrics();
            assert_eq!(cap.size().height, metrics.line_height + 2.0);
            assert_eq!(
                cap.size().width,
                text.len() as f32 * metrics.cell_width + 4.0,
                "keycap wraps the label's natural width plus two pixels per side",
            );
        }
        let text = painted(&runtime);
        assert!(text.contains("Ctrl\nShift\nR"));
        assert!(!text.contains('+'));
        assert!(!text.contains("app."));
        assert!(!text.contains("terminal."));
        assert!(
            !text.contains("No other tab"),
            "disabled reasons are not shown"
        );
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_uses_shared_local_blur_with_a_distinct_inset_editor() {
    for (width, height, scale) in [(1000, 700, 1.0), (1500, 1000, 1.5), (320, 700, 1.0)] {
        let (_tabs, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        let allocation = controller.latest_terminal_allocation();
        open(&controller, &mut runtime);
        assert_eq!(controller.latest_terminal_allocation(), allocation);

        let delta = runtime.pending_delta().unwrap();
        let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
        let shared = GlassAppearance::default();
        let palette = GlassAppearance::command_palette();
        let panel = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::BackdropMaterial {
                    rect,
                    sigma,
                    refraction,
                    material,
                    corner_radii,
                } => {
                    assert_eq!(Some(material), shared.material);
                    assert_eq!(sigma, shared.blur_sigma);
                    assert_eq!(refraction, shared.refraction);
                    assert_eq!(corner_radii, [12.0; 4]);
                    Some(rect)
                }
                _ => None,
            })
            .expect("palette must filter the real backdrop, not just paint translucency");
        assert_eq!(
            panel.size().width,
            620.0_f32.min(width as f32 / scale - 24.0)
        );
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::PaintedQuad { rect, paint, .. }
                if rect == panel && paint == shared.panel_fill)));
        let editor = items
            .iter()
            .find_map(|item| match item.primitive {
                Primitive::RoundedQuad {
                    rect,
                    color,
                    corner_radii,
                } if corner_radii == [7.0; 4] && color == palette.input_fill => Some(rect),
                _ => None,
            })
            .expect("editor has its own inset surface");
        assert!(palette.input_fill.a > shared.input_fill.a);
        assert!(palette.input_fill.a < 1.0, "editor remains translucent");
        assert_eq!(editor.min, Point::new(panel.min.x + 8.0, panel.min.y + 8.0));
        assert!(editor.max.x <= panel.max.x - 8.0);
        assert!(editor.size().width > 0.0);
        assert!(items.iter().any(|item| matches!(item.primitive,
            Primitive::RoundedBorder { rect, color, width: 1.0, corner_radii }
                if rect == editor && color == palette.input_border.unwrap().color()
                    && corner_radii == [7.0; 4])));
        assert!(painted(&runtime).contains("Type a command label or ID"));
        assert!(bytes.lock().unwrap().is_empty());
    }
}
