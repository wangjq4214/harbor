use super::*;

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn actual_conceal_overline_script_preserves_end_prompt_and_live_editing() {
    let mut live = LiveCmd::new(29);
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/terminal/verify_conceal_overline_windows.ps1")
        .canonicalize()
        .expect("actual repository conceal/overline script");
    live.run_powershell(
        &format!(
            "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"\r",
            windows_path(&script)
        ),
        END,
    );
    let input = live.pending_input();
    for (round, cols) in [51, 103, 51, 103, 51, 103].into_iter().enumerate() {
        live.resize(29, cols);
        let text = retained(&live.terminal);
        assert!(
            text.contains(END),
            "round={round}, cols={cols}: complete END lost: {text:?}"
        );
        live.assert_input(&input, &format!("script round={round}, cols={cols}"));
    }
    live.edit_and_execute(&input);
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn actual_script_long_ascii_and_cjk_pending_commands_survive_scroll_and_resize() {
    for payload in ["a".repeat(1000), "a界".repeat(100)] {
        let mut live = LiveCmd::new(29);
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/terminal/verify_conceal_overline_windows.ps1")
            .canonicalize()
            .unwrap();
        live.run_powershell(
            &format!(
                "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"\r",
                windows_path(&script),
            ),
            END,
        );
        let input = format!("echo {payload}");
        live.send(&input);
        let expected = format!("{}{input}", live.prompt);
        live.wait_for("long pending command", |t| {
            retained(t).trim_end().ends_with(&expected)
        });
        live.quiesce("long pending command echo");
        live.assert_input(&input, "long command before resize");
        for cols in [51, 103, 51, 103] {
            live.resize(29, cols);
            assert!(
                retained(&live.terminal).contains(END),
                "long command must not erase END"
            );
            live.assert_input(&input, "long command after resize");
        }
        live.edit_and_execute(&input);
    }
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_ordinary_tail_wraps_across_old_right_boundary() {
    producer_edge_fixture("AB", "'AB'", 140, "0");
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_background_tail_wraps_across_old_right_boundary() {
    producer_edge_fixture("AB", "'AB'", 140, "44");
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_wide_glyph_straddles_old_right_boundary() {
    producer_edge_fixture(
        &format!("{}界", "Z".repeat(102)),
        "('Z'*102)+[char]0x754C",
        65,
        "53;32",
    );
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_overline_tail_wraps_across_old_right_boundary() {
    producer_edge_fixture("AB", "'AB'", 140, "53;32");
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_underline_tail_wraps_across_old_right_boundary() {
    producer_edge_fixture("AB", "'AB'", 140, "4;31");
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_body_exactly_fills_new_right_boundary() {
    producer_edge_fixture(&"Z".repeat(51), "('Z'*51)", 65, "53;32");
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_wide_glyph_ends_at_new_right_boundary() {
    producer_edge_fixture(
        &format!("{}界", "Z".repeat(49)),
        "('Z'*49)+[char]0x754C",
        65,
        "4;31",
    );
}

#[test]
#[ignore = "pty: run the matching explicit test recipe"]
fn producer_wide_glyph_straddles_new_right_boundary() {
    producer_edge_fixture(
        &format!("{}界", "Z".repeat(50)),
        "('Z'*50)+[char]0x754C",
        65,
        "53;32",
    );
}

// Macro-generated cases also spawn real children: keep them in the serial PTY gate.
macro_rules! fixture_test {
    ($name:ident, $sgr:literal, $rows:literal) => {
        #[test]
        #[ignore = "pty: run the matching explicit test recipe"]
        fn $name() {
            minimal_fixture($sgr, $rows);
        }
    };
}

fixture_test!(overline_printed_and_erased_tails, "53;32", 29);
fixture_test!(underline_printed_and_erased_tails, "4;31", 29);
fixture_test!(background_printed_and_erased_tails, "44", 29);
fixture_test!(
    overline_tails_at_bottom_with_history_and_height_resize,
    "53;32",
    6
);
fixture_test!(
    underline_tails_at_bottom_with_history_and_height_resize,
    "4;31",
    6
);
fixture_test!(
    background_tails_at_bottom_with_history_and_height_resize,
    "44",
    6
);
