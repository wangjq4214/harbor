# Current-session search integration (T0003)

## Scope and invocation

This delivers [T0003](../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md#stable-slice-contracts-and-evidence) over the [bounded literal consumer](terminal-literal-search.md) and [read model](terminal-read-model.md), not all of spec 0027 or the parent search/shell work packages.

| Command ID | Default binding | Behavior |
| --- | --- | --- |
| `terminal.search` | Ctrl+Shift+F | Open/refocus the current-session query entry above the terminal. |
| `terminal.search-next` | F3 | Navigate next, oldest-first initially, wrapping at the end. |
| `terminal.search-previous` | Shift+F3 | Navigate previous, newest-first initially, wrapping at the beginning. |
| `terminal.search-close` | None globally | Close/cancel and restore terminal focus. |

Bindings use the existing structured configuration/registry/typed FIFO host dispatcher. Search shortcuts consume even when no matches/search are available; unbind them to pass those chords to the terminal. Escape **inside the query entry** closes; Enter/Shift+Enter navigate. There is no new global Escape binding. Previous/Next/Close buttons use the same dispatcher. Clicking the entry refocuses it through `terminal.search`; reopening preserves its query in the same session. Switching/creating/closing the active tab cancels search and closes its entry; no per-tab query archive or cross-session search is introduced.

The minimal editor appends printable input/IME commits, removes one Unicode scalar with Backspace, and accepts at most **128 UTF-8 bytes**. An over-limit/control-containing commit is ignored rather than silently searched as a truncated query. Preedit is display-only, bounded to 32 scalars, and cleared on commit/edit/focus loss. There is no caret movement, text selection, clipboard-paste integration or advanced editing in this entry. Clear the query with Backspace or close/reopen after cancellation. While focused, its keyboard/releases/IME never reach the PTY. Terminal focus and existing context-sensitive copy/interrupt and primary/alternate scroll pass-through remain unchanged outside the entry.

## Work, publication and lifetime

- `SessionSearch` is application-owned via `TabManager`; `TabCoordinator`/`ActiveSession` execute typed intents and advance work from `about_to_wait`. No worker thread, global event bus or generic business controller was added. Cooperative completion is asynchronous across host turns: a finished job is stored, then accepted on a later turn, allowing output/query/session changes between work and publication.
- Each turn performs at most one consumer page (**32 rows, 1024 units, 64 KiB copied payload**) or delivers one completion. Pending work requests another event-loop turn; it is never started by a frame/hover. The matcher captures a fixed retained-scope end, rebases compatible pages explicitly, and cannot chase a growing history tail indefinitely.
- At most one job **or** pending completion, one accepted batch, one bounded bridge range copy, one query string, and two gate tokens (working/presented) are retained. Each batch is capped at **1024 ranges**, each with at most 128 row proofs. No completion/query archive exists. Actual query edits/cancellation cancel both tokens and drop all work/results immediately. The already-published batch has its own gate so untouched positives remain navigable while same-query refresh work runs.
- Publication uses `LiteralSearch::accept` under the current engine lock: query provenance, engine/session identity, active buffer/generation and every retained-row text proof must agree. Old revisions cannot assert complete negative coverage. Stored navigation rechecks its presented query gate through `target`, then `navigate_content_target` validates again before scrolling. No parser/cursor/selection mutation or PTY write is part of search activation.
- Host tab identity and engine `SessionId` are both relevant. Tab switches/close cancel immediately; replacing an engine at the same tab ID fails the content gate. EOF alone does not invalidate retained search content.
- On content changes, published ranges are revalidated. A changed/lost row invalidates the batch; unchanged positives survive unrelated append with explicitly partial coverage. Once an in-flight fixed-scope job finishes, fresh bounded work covers the latest retained scope. Resize/reflow/reset/buffer replacement use deterministic invalidation and fresh work, not old-coordinate remapping. Alternate rectangular clipping and ordinary/ConPTY tail losses remain losses.
- The bar reports searching/incomplete, complete retained scope, or **partial/limited coverage**, with a match count. Result/read/position limits and output races never appear as complete no-match. A refresh may reset the selected result index when its new batch arrives; preserving navigation/result identity through reflow is not promised here.

## Neutral current-geometry highlights

The application publishes proven terminal-domain `ContentRange`s into `TerminalWidgetBridge`, and requests a frame. Every actual draw revalidates them against the locked terminal and projects only referenced **visible** rows with the shared logical-unit widths. No matching, source-string copying or full-history traversal occurs in paint/hover. Overlapping/adjacent cell spans merge. Logical ranges retain provenance; viewport `CellSpan`s are transient and never stored as navigation identity.

The concrete GPU layer caches vertices by spans/current `RenderViewport`, uses a translucent amber overlay after backgrounds and before glyphs, and does not alter source cells, SGR or parser state. Selection, cursor, preedit and scrollbar retain their existing layers. Both live and reused bridge draw paths refresh highlights; `read_update`/`acknowledge_update` ordering remains unchanged. The keyed terminal surface survives inserting/removing the bar, preserving focus/input ownership on close.

## T0003 criterion audit

| Criterion | Implementation and executed focused evidence |
| --- | --- |
| A1 — usable slice | Real workspace input → typed host dispatch → bounded matcher → bridge publication → next/previous scrolling; `current_session_search_entry_routes_typed_commands_without_pty_leakage`. Editor, limits and invocation are documented above. |
| A2 — acceptance/activation | `delayed_completion_and_activation_reject_overwrite_at_resolvable_positions` with mutation before publication and again after presentation, plus the real publication/projection/activation helpers. |
| A3 — session/query lifecycle | `old_query_cancellation_switch_close_and_replacement_completions_are_ineligible`; `buffers_reset_resize_erase_and_structural_edits_refresh_in_real_host_path`; `typed_host_dispatch_scopes_query_and_wraps_previous_next_navigation`. These exercise the same `TabManager` execution/gates used by the native host, not a second matcher. |
| A4 — projection/neutrality | Three `read_model::presentation_tests` cover multi-page wraps versus hard breaks, complete CJK/combining/emoji units, padding/cues, geometry changes and pending damage/events. GPU pixel readback proves current-position amber cells, cached frame equivalence and stale removal; the existing all-layer pipeline test now encodes highlights alongside selection/preedit/cursor. |
| A5 — commands/ownership | Workspace routing verifies query/IME/navigation emit zero PTY bytes and ordinary typing resumes on close. Existing `terminal_shortcuts_enqueue_only_when_context_consumes`, native Escape and command rebinding regressions pass. No widget business logic or OSC 7/8/133 parser/activation policy changes. |
| A6 — progress/invalidation | Host tests cover append during multi-page work and navigation through subsequent refreshes, overwrite, erase/structural edits, width/height/reflow, alternate/saved-primary resize, partial/whole eviction/ring reuse and engine replacement. Per-turn copy bounds and steady-state absence of jobs are asserted. `application_exposes_result_limits_and_releases_work_on_empty_or_invalid_queries` covers coverage caps, cancellation and close ownership. |

## Verification snapshot and results

Base revision: `88822d2177278d8c1300986c2a3148df33e98e52` plus this uncommitted T0003 implementation. Changed surfaces: `crates/harbor-app/src/{command,lib,search,search_tests,tab_manager,terminal_view,workspace,workspace_tests}.rs`, `crates/harbor-terminal/src/read_model.rs`, `read_model/presentation_tests.rs`, `render/{mod,pipeline,highlight,highlight_tests}.rs`, root `src/{shell,tab_coordinator}.rs`, this document and scoped ticket/set updates. The local ignored plan is `.grimoire/plans/0036-current-session-search-integration.md`.

Environment: Windows, Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`, Python `3.12.10`. Commands run from the repository root.

| Gate | Result |
| --- | --- |
| `cargo test --locked -q -p harbor-app search` | PASS — 9 focused application tests. |
| `cargo test -p harbor-terminal read_model::presentation_tests` | PASS — 3 core projection tests. |
| `cargo test -p harbor-terminal gpu_highlights -- --nocapture` | PASS — actual GPU pixel readback, not an adapter-unavailable skip. |
| `cargo test -p harbor-terminal gpu_pipeline_prepares_replayed` | PASS — all-layer GPU preparation/encode including highlights. |
| `cargo test --locked -q -p harbor-terminal --no-default-features` | PASS — 743 active core unit tests, existing ignored/manual tests unchanged, active integration tests and both API doctests. |
| `cargo test --locked --workspace -q` | PASS on final full rerun — 128 application and 1064 active terminal unit tests, plus remaining active workspace/integration/doctest suites; existing ignored/manual tests unchanged. |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS. |
| `cargo fmt --all -- --check`, `git diff --check` | PASS. |
| `python scripts/check_docs.py`, `python scripts/checklist_summary.py` | PASS. |

The first workspace invocation failed one existing GPU comparison, `t0002_native_changed_metrics_replacement_and_failed_preparation_are_coherent` in `render/text_tests.rs`, at same-metrics font-replacement pixel equality. It passed in isolation and the complete workspace rerun passed without code changes. The intermittent cause remains unproven (not asserted to be a baseline defect); no unrelated font/test behavior was changed to obtain the pass. Non-fatal incremental-cache access-denied notes did not fail the builds.

Early regression tests exposed and repaired terminal-focus loss on bar removal: the unkeyed conditional child displaced the retained terminal subtree. The keyed surface fixes the original consumed-input/close/normal-typing regression. Fixture expectations were also corrected for navigation wrap order and rectangular alternate-screen clipping; discarded text is not restored to satisfy a test.

Inline review and criterion audit found no confirmed T0003 blocker; A1–A6 have scoped automated evidence above. Planning was useful for coordinating the host/session, query gate and current-geometry seams. No delegated work or human checkpoint was added. Native runtime evidence and investigation of the intermittent existing GPU comparison remain separate from this automated result.

Review/check scope includes production changes, added tests and documentation. No full #159/#160 delivery, new OSC behavior or universal renderer boundary is implied. **T0004 Windows interactive ongoing-output/resize/search evidence and large-history work/memory measurements remain NOT RUN here**. Automated Windows/headless/GPU results are not a native GUI/runtime acceptance claim, and do not complete spec 0027.
