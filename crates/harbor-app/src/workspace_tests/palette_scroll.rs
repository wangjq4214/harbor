use super::palette_support::*;
use super::*;

#[test]
fn palette_results_leave_a_stable_gutter_outside_row_surfaces() {
    for (width, height, scale) in [(1000, 500, 1.0), (1500, 750, 1.5), (320, 500, 1.0)] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(width, height, scale));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (row, viewport) = selected_result_viewport(&runtime);
        let thumb = palette_thumb(&runtime).expect("overflow scrollbar");
        assert!(row.max.x < thumb.min.x, "idle thumb must not cover a row");

        row_pointer(
            &mut runtime,
            Point::new(viewport.max.x - 5.0, row.min.y + 10.0),
            PointerPhase::Move,
        );
        let delta = runtime.pending_delta().unwrap();
        let track = delta
            .added
            .iter()
            .chain(&delta.modified)
            .find_map(|item| match item.primitive {
                Primitive::Quad {
                    rect,
                    color,
                    corner_radius: 3.0,
                } if color.a == 0.06 => Some(rect),
                _ => None,
            })
            .expect("hover expands the scrollbar track");
        assert!(
            row.max.x + 4.0 <= track.min.x,
            "the entire scrollbar hit target stays beyond the row background and its contents"
        );
        assert!(
            row_surface(&runtime, 0.06).is_none(),
            "gutter is not a row hover target"
        );
        assert_eq!(runtime.input().focused(), focus);

        replace(&mut runtime, "app.new-tab");
        let (filtered_row, filtered_viewport) = selected_result_viewport(&runtime);
        assert_eq!(filtered_row.size().width, row.size().width);
        assert_eq!(filtered_viewport.size().width, viewport.size().width);
        assert!(palette_thumb(&runtime).is_none());
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());
    }
}

#[test]
fn palette_real_results_scroll_drag_and_keyboard_reveal_keep_query_focus_and_pty_quiet() {
    for height in [500, 180] {
        let (_, controller, mut runtime, bytes) = fixture();
        runtime.set_viewport(Viewport::new(1000, height, 1.0));
        runtime.update(Instant::now());
        open(&controller, &mut runtime);
        let focus = runtime.input().focused();
        let (first, viewport) = selected_result_viewport(&runtime);
        let text = painted(&runtime);
        assert!(text.contains("Reload configuration"));
        assert!(
            text.contains("Previous search match"),
            "offscreen results exist, not a five-item slice"
        );
        assert!(palette_thumb(&runtime).is_some());
        assert!(first.min.y >= viewport.min.y && first.max.y <= viewport.max.y);

        runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
            Point::new(viewport.min.x + 20.0, viewport.min.y + 10.0),
            PointerPhase::WheelPixel {
                dx: 0.0,
                dy: -100.0,
            },
            PointerButton::Left,
            10,
        )));
        runtime.update(Instant::now());
        let (scrolled, _) = selected_result_viewport(&runtime);
        assert_eq!(
            scrolled.min.y,
            first.min.y - 100.0,
            "continuous pixel scrolling"
        );
        assert_eq!(runtime.input().focused(), focus);
        let thumb = palette_thumb(&runtime).unwrap();
        for (phase, y) in [
            (PointerPhase::Down, thumb.min.y + 2.0),
            (PointerPhase::Move, viewport.max.y + 100.0),
            (PointerPhase::Up, viewport.max.y + 100.0),
        ] {
            runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
                Point::new(thumb.min.x + 2.0, y),
                phase,
                PointerButton::Left,
                11,
            )));
            runtime.update(Instant::now());
        }
        assert!(runtime.input().captor(11).is_none());
        assert_eq!(runtime.input().focused(), focus);
        tap(&mut runtime, Key::ArrowDown);
        runtime.update(Instant::now());
        let (selected, clipped) = selected_result_viewport(&runtime);
        assert!(selected.min.y >= clipped.min.y && selected.max.y <= clipped.max.y);
        assert_eq!(runtime.input().focused(), focus);
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());

        replace(&mut runtime, "app.new-tab");
        let (selected, clipped) = selected_result_viewport(&runtime);
        assert_eq!(
            selected.min.y, clipped.min.y,
            "filter resets to the first result"
        );
        assert!(
            palette_thumb(&runtime).is_none(),
            "one result has no scrollbar"
        );
        assert_eq!(runtime.input().focused(), focus);
        replace(&mut runtime, "not-a-real-command");
        assert!(painted(&runtime).contains("No matching commands"));
        assert!(palette_thumb(&runtime).is_none());
        assert!(controller.drain_actions().is_empty());
        assert!(bytes.lock().unwrap().is_empty());
    }
}
