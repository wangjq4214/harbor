use super::*;

#[test]
fn bracketed_paste_mode_resets_and_is_scoped_to_active_screen() {
    let mut terminal = Terminal::new_headless(1, 1);

    terminal.put_bytes(b"\x1b[?2004h\x1bc");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[!p");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[?1049h");
    assert!(!terminal.screen().input_modes().bracketed_paste);

    terminal.put_bytes(b"\x1b[?2004h\x1b[?1049l");
    assert!(terminal.screen().input_modes().bracketed_paste);
}

#[test]
fn paste_without_bracketed_mode_preserves_raw_bytes() {
    let modes = InputModes::default();
    let bytes = modes.paste(b"first\r\nsecond\x1b[A");

    assert!(matches!(bytes, Cow::Borrowed(_)));
    assert_eq!(bytes.as_ref(), b"first\r\nsecond\x1b[A");
}

#[test]
fn paste_with_bracketed_mode_frames_multiline_content() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(
        modes.paste(b"first\r\nsecond\x1b[A").as_ref(),
        b"\x1b[200~first\r\nsecond\x1b[A\x1b[201~"
    );
}

#[test]
fn paste_with_bracketed_mode_frames_empty_content() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(modes.paste(b"").as_ref(), b"\x1b[200~\x1b[201~");
}

#[test]
fn paste_with_bracketed_mode_retains_end_marker_for_large_content() {
    let text = vec![b'x'; 1024 * 1024];
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };

    let bytes = modes.paste(&text);
    assert_eq!(
        bytes.len(),
        text.len() + b"\x1b[200~".len() + b"\x1b[201~".len()
    );
    assert_eq!(&bytes[..b"\x1b[200~".len()], b"\x1b[200~");
    assert_eq!(
        &bytes[b"\x1b[200~".len()..b"\x1b[200~".len() + text.len()],
        text.as_slice()
    );
    assert_eq!(&bytes[text.len() + b"\x1b[200~".len()..], b"\x1b[201~");
}

#[test]
fn multiline_empty_is_not_multiline() {
    assert!(!should_confirm_multiline(""));
}

#[test]
fn multiline_single_line_is_not_multiline() {
    assert!(!should_confirm_multiline("hello"));
}

#[test]
fn multiline_single_line_with_trailing_lf_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\n"));
}

#[test]
fn multiline_single_line_with_trailing_crlf_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\r\n"));
}

#[test]
fn multiline_single_line_with_multiple_trailing_newlines_is_not_multiline() {
    assert!(!should_confirm_multiline("hello\n\n\n"));
    assert!(!should_confirm_multiline("hello\r\n\r\n"));
    assert!(!should_confirm_multiline("hello\n\r\n"));
}

#[test]
fn multiline_two_lines_is_multiline() {
    assert!(should_confirm_multiline("hello\nworld"));
}

#[test]
fn multiline_two_lines_with_trailing_lf_is_multiline() {
    assert!(should_confirm_multiline("hello\nworld\n"));
}

#[test]
fn multiline_windows_crlf_is_multiline() {
    assert!(should_confirm_multiline("hello\r\nworld"));
}

#[test]
fn multiline_windows_crlf_with_trailing_crlf_is_multiline() {
    assert!(should_confirm_multiline("hello\r\nworld\r\n"));
}

#[test]
fn multiline_three_lines_is_multiline() {
    assert!(should_confirm_multiline("a\nb\nc"));
}

#[test]
fn multiline_only_newlines_is_not_multiline() {
    assert!(!should_confirm_multiline("\n"));
    assert!(!should_confirm_multiline("\n\n\n"));
    assert!(!should_confirm_multiline("\r\n\r\n"));
}

#[test]
fn multiline_mixed_line_endings_is_multiline() {
    assert!(should_confirm_multiline("a\r\nb\nc"));
    // After trimming trailing \r\n, the remaining "a" has no newline
    assert!(!should_confirm_multiline("a\r\n"));
    // After trimming trailing \n, "a\r" has the CR which is a line break
    assert!(should_confirm_multiline("a\rb\n"));
}

#[test]
fn disposition_bracketed_paste_on_sends_direct() {
    let modes = InputModes {
        bracketed_paste: true,
        ..InputModes::default()
    };
    assert_eq!(
        PasteDisposition::decide(modes, "hello\nworld"),
        PasteDisposition::SendDirect
    );
    // Single-line with BP on also SendDirect
    assert_eq!(
        PasteDisposition::decide(modes, "hello"),
        PasteDisposition::SendDirect
    );
}

#[test]
fn disposition_multiline_bp_off_is_confirm() {
    let modes = InputModes::default(); // bracketed_paste: false
    let disposition = PasteDisposition::decide(modes, "hello\nworld");
    assert_eq!(
        disposition,
        PasteDisposition::Confirm {
            raw_text: "hello\nworld".to_owned()
        }
    );
}

#[test]
fn disposition_single_line_bp_off_is_send_direct() {
    let modes = InputModes::default();
    assert_eq!(
        PasteDisposition::decide(modes, "hello"),
        PasteDisposition::SendDirect
    );
    // Single line + trailing newline
    assert_eq!(
        PasteDisposition::decide(modes, "hello\n"),
        PasteDisposition::SendDirect
    );
}

#[test]
fn disposition_confirm_preserves_raw_text() {
    let modes = InputModes::default();
    let text = "line1\r\nline2\twith\ttabs\nline3";
    let disposition = PasteDisposition::decide(modes, text);
    assert_eq!(
        disposition,
        PasteDisposition::Confirm {
            raw_text: text.to_owned()
        }
    );
}

#[test]
fn preview_plain_text_is_unchanged() {
    assert_eq!(safe_preview_line("hello world"), "hello world");
}

#[test]
fn preview_empty_string_is_empty() {
    assert_eq!(safe_preview_line(""), "");
}

#[test]
fn preview_tab_becomes_visible_marker() {
    let result = safe_preview_line("a\tb");
    assert!(result.contains('\u{2192}')); // →
    assert!(!result.contains('\t'));
}

#[test]
fn preview_esc_becomes_visible_marker() {
    let result = safe_preview_line("a\x1bb");
    assert!(!result.contains('\x1b'));
    assert!(result.len() > 3); // marker should have been inserted
}

#[test]
fn preview_cr_and_lf_pass_through() {
    // CR and LF are line-break delimiters handled by the caller;
    // safe_preview_line receives pre-split lines and should not escape them.
    assert_eq!(safe_preview_line("a\rb"), "a\rb");
    assert_eq!(safe_preview_line("a\nb"), "a\nb");
}

#[test]
fn preview_null_becomes_visible_marker() {
    let result = safe_preview_line("a\x00b");
    assert!(!result.contains('\x00'));
    assert_ne!(result, "a\x00b");
}

#[test]
fn preview_del_becomes_visible_marker() {
    let result = safe_preview_line("a\x7fb");
    assert!(!result.contains('\x7f'));
    assert_ne!(result, "a\x7fb");
}

#[test]
fn preview_multiple_controls_in_one_line() {
    let result = safe_preview_line("\t\x1b\x00");
    assert!(!result.contains('\t'));
    assert!(!result.contains('\x1b'));
    assert!(!result.contains('\x00'));
    // Each control char should be replaced by a visible marker
    assert!(result.len() >= 3);
}

#[test]
fn preview_cjk_text_is_unchanged() {
    let cjk = "你好世界";
    assert_eq!(safe_preview_line(cjk), cjk);
}

#[test]
fn preview_printable_ascii_range_is_unchanged() {
    let printable: String = (b' '..=b'~').map(|b| b as char).collect();
    assert_eq!(safe_preview_line(&printable), printable);
}
