use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn rendered_bridge_replays_hidden_output_and_retained_resize_after_skipped_frames() {
    assert_rendered_bridge_replays_hidden_output_and_retained_resize(None);
    // Exercise the geometry that previously exposed styled scroll-fill tails
    // on CI, independently of the machine's selected system font.
    assert_rendered_bridge_replays_hidden_output_and_retained_resize(Some(TextMetrics {
        line_height: 30.0,
        ..metrics()
    }));
}
