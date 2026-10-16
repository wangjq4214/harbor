use super::*;
use crate::{
    command::AppCommand, tab_manager::TerminalTabResources, workspace::WorkspaceUiController,
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

fn create(tabs: &mut TabManager, rows: usize, cols: usize) -> TabId {
    tabs.create_tab(|_, draw| {
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(Mutex::new(Terminal::new_headless(rows, cols)));
        let bridge =
            TerminalWidgetBridge::new(draw, terminal.clone(), Arc::new(AtomicBool::new(false)));
        Ok(TerminalTabResources::new(terminal, bridge, "test"))
    })
    .unwrap();
    tabs.active_id().unwrap()
}
fn setup(text: &str) -> TabManager {
    let mut tabs = TabManager::new();
    create(&mut tabs, 3, 8);
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str(text);
    tabs
}
fn complete(tabs: &mut TabManager) {
    for _ in 0..2000 {
        if !tabs.advance_search().pending {
            return;
        }
    }
    panic!("search failed to settle");
}
fn pending(tabs: &mut TabManager) {
    for _ in 0..2000 {
        tabs.advance_search();
        if tabs.search.completion.is_some() {
            return;
        }
    }
    panic!("completion missing");
}
fn published(tabs: &TabManager) -> usize {
    tabs.active_bridge().unwrap().decoration_ranges().len()
}
fn ui(tabs: &TabManager) -> WorkspaceUiController {
    WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0)
}

#[test]
fn delayed_completion_and_activation_reject_overwrite_at_resolvable_positions() {
    let mut tabs = setup("needle\r\nother");
    tabs.set_search_query("needle");
    pending(&mut tabs);
    let old = tabs.search.completion.as_ref().unwrap().ranges()[0].clone();
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("\x1b[1;1HX");
    tabs.advance_search();
    assert_eq!(published(&tabs), 0);
    assert!(tabs.search.accepted.is_none());
    assert!(!tabs.navigate_search(false));
    complete(&mut tabs);
    assert_eq!(published(&tabs), 0);
    let terminal = tabs.active_terminal().unwrap();
    terminal.lock().unwrap().put_str("\x1b[1;1Hneedle");
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
    terminal.lock().unwrap().put_str("\x1b[1;1HX");
    assert!(
        !tabs.navigate_search(false),
        "revalidate immediately, before the next host tick"
    );
    assert_eq!(published(&tabs), 0);
    assert!(!terminal.lock().unwrap().validate_content_range(&old));
}

#[test]
fn old_query_cancellation_switch_close_and_replacement_completions_are_ineligible() {
    let mut tabs = setup("needle other");
    let first = tabs.active_id().unwrap();
    let first_bridge = tabs.active_bridge().unwrap();
    tabs.set_search_query("needle");
    pending(&mut tabs);
    let old = tabs.search.completion.take().unwrap();
    tabs.set_search_query("other");
    tabs.search.completion = Some(old); // delayed delivery after replacement
    tabs.advance_search();
    assert_eq!(published(&tabs), 0);
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
    tabs.set_search_query("needle");
    pending(&mut tabs);
    let old = tabs.search.completion.take().unwrap();
    tabs.cancel_search();
    tabs.search.completion = Some(old);
    tabs.advance_search();
    assert_eq!(published(&tabs), 0);
    assert!(!tabs.navigate_search(false));
    let second = create(&mut tabs, 3, 8);
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("needle");
    tabs.activate(first);
    tabs.set_search_query("needle");
    pending(&mut tabs);
    let old = tabs.search.completion.take().unwrap();
    tabs.activate(second);
    assert!(first_bridge.decoration_ranges().is_empty());
    tabs.set_search_query("needle");
    tabs.search.completion = Some(old);
    tabs.advance_search();
    assert_eq!(
        published(&tabs),
        0,
        "identical text in another tab is not identity"
    );
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
    tabs.close(second);
    assert_eq!(tabs.active_id(), Some(first));
    assert!(!tabs.navigate_search(false));
    assert!(tabs.search.job.is_none());
    tabs.set_search_query("needle");
    pending(&mut tabs);
    *tabs.active_terminal().unwrap().lock().unwrap() = Terminal::new_headless(3, 8);
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("needle");
    tabs.advance_search();
    assert_eq!(published(&tabs), 0, "replaced engine at the same tab ID");
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
}

#[test]
fn buffers_reset_resize_erase_and_structural_edits_refresh_in_real_host_path() {
    for mutation in [
        "\x1b[?47h",
        "\x1bc",
        "\x1b[1;1H\x1b[2K",
        "\x1b[1;1H\x1b[M",
        "\x1b[1;1H\x1b[@",
    ] {
        let mut tabs = setup("needle\r\nother");
        tabs.set_search_query("needle");
        complete(&mut tabs);
        assert_eq!(published(&tabs), 1);
        tabs.active_terminal()
            .unwrap()
            .lock()
            .unwrap()
            .put_str(mutation);
        assert!(!tabs.navigate_search(false), "mutation {mutation:?}");
        complete(&mut tabs);
        let terminal = tabs.active_terminal().unwrap();
        for range in tabs.active_bridge().unwrap().decoration_ranges() {
            assert!(terminal.lock().unwrap().validate_content_range(&range));
        }
    }
    let mut tabs = setup("needle\r\nother");
    tabs.set_search_query("needle");
    pending(&mut tabs);
    let terminal = tabs.active_terminal().unwrap();
    terminal.lock().unwrap().put_str("\x1b[?47hneedle");
    terminal.lock().unwrap().resize(4, 4); // saved primary is resized too
    tabs.advance_search();
    assert_eq!(published(&tabs), 0);
    complete(&mut tabs);
    assert_eq!(
        published(&tabs),
        0,
        "rectangular alternate resize discards clipped tails"
    );
    terminal.lock().unwrap().put_str("\x1b[Hneedle");
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
    terminal.lock().unwrap().put_str("\x1b[?47l");
    assert!(!tabs.navigate_search(false));
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1);
    assert!(tabs.navigate_search(false));
    for size in [(4, 6), (5, 6), (3, 8)] {
        terminal.lock().unwrap().resize(size.0, size.1);
        assert!(!tabs.navigate_search(false));
        complete(&mut tabs);
        assert_eq!(published(&tabs), 1);
    }
}

#[test]
fn partial_eviction_whole_eviction_and_ring_reuse_never_keep_targets() {
    let mut tabs = TabManager::new();
    create(&mut tabs, 2, 3);
    let terminal = tabs.active_terminal().unwrap();
    terminal.lock().unwrap().put_str(&"abc".repeat(1002));
    tabs.set_search_query("abcabc");
    complete(&mut tabs);
    assert!(published(&tabs) > 0);
    terminal.lock().unwrap().put_str("mnopqr");
    assert!(!tabs.navigate_search(false));
    complete(&mut tabs);
    assert!(
        tabs.navigate_search(false),
        "fresh retained suffix remains useful"
    );
    terminal.lock().unwrap().put_str(&"new\r\n".repeat(1005));
    assert!(!tabs.navigate_search(false));
    complete(&mut tabs);
    assert_eq!(published(&tabs), 0);
}

#[test]
fn bounded_host_turns_make_useful_progress_while_output_continues() {
    let mut tabs = setup(&"needle\r\n".repeat(200));
    tabs.set_search_query("needle");
    let terminal = tabs.active_terminal().unwrap();
    let mut useful = false;
    let mut pages = 0;
    for _ in 0..100 {
        let tick = tabs.advance_search();
        if let Some(job) = &tabs.search.job {
            let work = job.last_work();
            assert!(work.rows_visited <= 32);
            assert!(work.copied_units <= 1024);
            assert!(work.copied_payload_bytes <= 64 * 1024);
            pages += 1;
        }
        if published(&tabs) > 0 {
            assert!(tabs.search_status().contains("partial"));
            assert!(tabs.navigate_search(false));
            useful = true;
            break;
        }
        terminal.lock().unwrap().put_str("out\r\n");
        assert!(tick.pending);
    }
    assert!(pages > 1);
    assert!(useful, "must not endlessly restart on append");
    // Once published, untouched positive results remain navigable THROUGH the
    // next multi-page refresh, not just for the completion's single host turn.
    terminal.lock().unwrap().put_str("more\r\n");
    for _ in 0..4 {
        tabs.advance_search();
        assert!(published(&tabs) > 0);
        assert!(tabs.navigate_search(false));
        terminal.lock().unwrap().put_str("out\r\n");
    }
    complete(&mut tabs);
    let observed = tabs.search.observed;
    for _ in 0..10 {
        assert!(!tabs.advance_search().pending);
        assert!(
            tabs.search.job.is_none(),
            "steady state does not rescan history"
        );
        assert_eq!(tabs.search.observed, observed);
    }
}

#[test]
fn typed_host_dispatch_scopes_query_and_wraps_previous_next_navigation() {
    let mut tabs = setup("needle\r\nneedle\r\nneedle\r\nlive");
    let ui = ui(&tabs);
    assert_eq!(
        tabs.dispatch_search_command(AppCommand::SearchOpen, &ui),
        Some(ui.search_focus())
    );
    tabs.set_search_query("needle");
    complete(&mut tabs);
    for command in [
        AppCommand::SearchNext,
        AppCommand::SearchPrevious,
        AppCommand::SearchPrevious,
        AppCommand::SearchNext,
    ] {
        tabs.dispatch_search_command(command, &ui);
        assert!(tabs.search.selected.is_some());
    }
    assert_eq!(tabs.search.selected, Some(2));
    tabs.dispatch_search_command(AppCommand::SearchClose, &ui);
    assert!(ui.search_query().is_none());
    assert_eq!(published(&tabs), 0);
    let old_ui = ui.clone();
    create(&mut tabs, 3, 8);
    old_ui.open_search(TabId(1));
    tabs.dispatch_search_command(AppCommand::SearchChanged, &old_ui);
    assert!(tabs.search.source.is_none());
}

#[test]
fn fast_search_starts_immediately_without_flashing_busy_or_leaving_a_timer() {
    let mut tabs = setup("a a");
    tabs.set_search_query("a");
    assert!(tabs.search.job.is_some(), "no input debounce");
    assert_eq!(tabs.search_status(), "No Result");
    let start = tabs.search.busy_since.unwrap();
    for _ in 0..2000 {
        let tick = tabs.advance_search_at(start + SEARCH_BUSY_DELAY / 2);
        assert!(!tabs.search_status().contains("Searching"));
        if !tick.pending {
            break;
        }
    }
    assert_eq!(tabs.search_status(), "— / 2");
    assert!(tabs.search.busy_since.is_none());
    let idle = tabs.advance_search_at(start + Duration::from_secs(10));
    assert!(!idle.pending && !idle.changed);
    assert_eq!(tabs.search_status(), "— / 2");
}

#[test]
fn slow_search_reveals_busy_at_200ms_and_requests_a_frame_without_more_input() {
    let mut tabs = setup(&"a ".repeat(10_000));
    tabs.set_search_query("a");
    let start = tabs.search.busy_since.unwrap();
    let first = tabs.advance_search_at(start + Duration::from_millis(199));
    assert!(first.pending);
    assert!(
        !first.changed,
        "the unchanged initial label needs no extra redraw"
    );
    assert_eq!(tabs.search_status(), "No Result");
    let delayed = tabs.advance_search_at(start + SEARCH_BUSY_DELAY);
    assert!(delayed.pending);
    assert!(
        delayed.changed,
        "host must request a frame when only the hint changes"
    );
    assert!(tabs.search_status().starts_with("Searching"));
    complete(&mut tabs);
    assert!(!tabs.search_status().contains("Searching"));
    assert!(tabs.search.busy_since.is_none());
}

#[test]
fn busy_hint_resets_on_query_replacement_empty_close_and_session_switch() {
    let mut tabs = setup(&"a ".repeat(10_000));
    let first_tab = tabs.active_id().unwrap();
    tabs.set_search_query("a");
    let old_start = tabs.search.busy_since.unwrap();
    tabs.advance_search_at(old_start + SEARCH_BUSY_DELAY);
    assert!(tabs.search_status().starts_with("Searching"));
    tabs.set_search_query("missing");
    assert_eq!(
        tabs.search_status(),
        "No Result",
        "old query count/hint must not leak"
    );
    let new_start = tabs.search.busy_since.unwrap();
    tabs.advance_search_at(new_start + Duration::from_millis(199));
    assert_eq!(tabs.search_status(), "No Result");
    tabs.set_search_query("");
    assert_eq!(tabs.search_status(), "No Result");
    assert!(tabs.search.busy_since.is_none());
    tabs.advance_search_at(new_start + Duration::from_secs(5));
    assert_eq!(tabs.search_status(), "No Result");
    tabs.set_search_query("a");
    tabs.cancel_search();
    assert!(tabs.search.busy_since.is_none());
    let second_tab = create(&mut tabs, 3, 8);
    tabs.activate(first_tab);
    tabs.set_search_query("a");
    tabs.activate(second_tab);
    tabs.advance_search();
    assert!(tabs.search.busy_since.is_none());
    assert_eq!(tabs.search_status(), "No Result");
}

#[test]
fn result_presentation_tracks_navigation_without_selecting_the_first_match() {
    let mut tabs = setup("a a a");
    assert_eq!(tabs.search_status(), "No Result");
    tabs.set_search_query("a");
    assert_eq!(tabs.search_status(), "No Result");
    complete(&mut tabs);
    assert_eq!(tabs.search.selected, None);
    assert_eq!(tabs.search_status(), "— / 3");
    let ui = ui(&tabs);
    tabs.dispatch_search_command(AppCommand::SearchNext, &ui);
    assert_eq!(tabs.search_status(), "1 / 3");
    tabs.navigate_search(true);
    assert_eq!(tabs.search_status(), "3 / 3");
    tabs.navigate_search(false);
    assert_eq!(tabs.search_status(), "1 / 3");
    tabs.set_search_query("not here");
    assert!(!tabs.search_status().contains("/ 3"));
    complete(&mut tabs);
    assert_eq!(tabs.search_status(), "No Result");
    assert_eq!(tabs.search.selected, None);
    tabs.set_search_query("");
    assert_eq!(tabs.search_status(), "No Result");
}

#[test]
fn invalidated_result_position_is_not_published_while_refreshing() {
    let mut tabs = setup("needle");
    tabs.set_search_query("needle");
    complete(&mut tabs);
    assert!(tabs.navigate_search(false));
    assert_eq!(tabs.search_status(), "1 / 1");
    tabs.active_terminal()
        .unwrap()
        .lock()
        .unwrap()
        .put_str("\x1b[1;1HX");
    assert!(!tabs.navigate_search(false));
    assert_eq!(tabs.search_status(), "No Result");
    assert_eq!(tabs.search.selected, None);
    complete(&mut tabs);
    assert_eq!(tabs.search_status(), "No Result");
}

#[test]
fn application_exposes_result_limits_and_releases_work_on_empty_or_invalid_queries() {
    let mut tabs = setup(&"a".repeat(1500));
    tabs.set_search_query("a");
    complete(&mut tabs);
    assert_eq!(published(&tabs), 1024);
    assert!(tabs.search_status().contains("partial/limited"));
    assert!(!tabs.search_status().contains("No Result"));
    assert!(tabs.search_status().contains("— / 1024"));
    assert!(tabs.navigate_search(false));
    assert!(tabs.search_status().contains("1 / 1024"));
    assert!(tabs.search_status().starts_with("partial/limited"));
    tabs.set_search_query(&"b".repeat(129));
    assert_eq!(published(&tabs), 0);
    assert!(tabs.search.job.is_none());
    assert!(tabs.search_status().contains("exceeds"));
    tabs.set_search_query("a");
    let bridge = tabs.active_bridge().unwrap();
    let terminal = tabs.active_terminal().unwrap();
    let weak = Arc::downgrade(&terminal);
    tabs.close_active();
    assert!(tabs.search.job.is_none());
    assert!(tabs.search.completion.is_none());
    assert!(tabs.search.accepted.is_none());
    assert!(bridge.decoration_ranges().is_empty());
    drop(bridge);
    drop(terminal);
    assert!(
        weak.upgrade().is_none(),
        "closed search retains no terminal or bridge"
    );
    let mut tabs = setup("a");
    tabs.set_search_query("a");
    tabs.set_search_query("");
    assert_eq!(published(&tabs), 0);
    assert!(!tabs.navigate_search(false));
    assert_eq!(tabs.search_status(), "No Result");
}
