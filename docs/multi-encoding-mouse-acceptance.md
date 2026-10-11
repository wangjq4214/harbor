# Multi-Encoding Mouse Reporting: Scoped Evidence

## Scope and environment

- Contract: [Spec 0021](../.grimoire/spec/0021-multi-encoding-mouse-reporting.md), [ADR 0055](../.grimoire/adr/0055-multi-encoding-mouse-reporting.md).
- Date: 2026-10-11.
- Revision: `6487d8f` plus the uncommitted multi-encoding mouse implementation/tests and checklist changes. Existing context/ADR/spec authoring changes are retained. No commit or staging was performed.
- Environment: Windows `10.0.26300.0`, installed `conhost.exe` file version `10.0.26100.8875 (WinBuild.160101.0800)`, Rust/Cargo `1.97.1`, Python `3.12.10`. This is an environment inventory, not proof of a running application/ConPTY configuration.
- Automated scope: terminal parser/input/coordinate/lifecycle tests with a recording PTY writer, encoder byte tests, and existing widget/app routing regressions. No native GUI mouse workload was launched for this evidence record.

## Implemented policies

Tracking and encoding flags are independent, with tracking priority `1003 > 1002 > 1000` and encoding priority `1016 > 1006 > 1015 > 1005 > legacy`. Queries report each retained flag. Unsupported `9` stays unknown. Buffer-local lifetimes are unchanged.

Legacy uses single-byte numeric offsets and drops the entire report above final coordinate 223. UTF-8 encodes the offset numeric fields and drops above final coordinate 2015. Decimal formats do not inherit those limits. Non-finite or final coordinates exceeding `u32::MAX` are dropped rather than wrapped. Cell viewport clamping is preserved. Pixel coordinates are content-relative physical positions: scale once, subtract physical allocation origin/padding, floor, add one. Negative finite pixels clamp to the content origin; upper pixels are not viewport-clamped to a different position. These boundaries do not enable local selection/scrollback while tracking is active.

SGR horizontal wheels use positive dx → 66 (left), negative dx → 67 (right). Dual-axis output order is **vertical then horizontal**, one report for each nonzero axis, independent of magnitude and wheel increment units. No accumulation or new vertical quantization is introduced. Non-finite wheel axes produce no report for that axis.

## Requirement-to-evidence map

All new terminal tests are in `crates/harbor-terminal/src/mouse_tests.rs` and run without the renderer feature.

| Requirement | Named automated evidence | Boundary established |
| --- | --- | --- |
| R1/R5 independent modes, queries and fallback | `mouse_encoding_flags_queries_fallback_ris_and_tracking_off`; existing `should_track_mouse_modes_independently_across_multi_parameter_changes_and_ris` | Enable/disable idempotence, retained flags/queries in different orders, encoding priority, tracking fallback, unsupported `9`, RIS, encoding-only local selection |
| R1 coherent pending output | `mouse_queued_output_resolves_tracking_and_coordinate_units_before_projection` | Reader-queued tracking enable/disable and cell↔pixel changes are parsed before routing/projection; exact PTY bytes |
| R2 wire formats | `mouse_all_encodings_press_release_reach_pty`, `mouse_all_buttons_modifiers_and_event_codes` | Exact press/release/motion/wheel bytes for every encoding, all three buttons and 16 modifier combinations including Alt+Meta sharing one bit |
| R2 range safety | `mouse_encoding_numeric_boundaries_are_exact_and_safe`, `mouse_large_viewport_limits_are_not_hidden_by_cell_clamping` | Legacy 223/224, UTF-8 95/96/128/2015/2016, both axes, decimal >223 and large finite coordinates, non-finite/overflow drop |
| R3 geometry/units | `mouse_pixel_geometry_precision_switching_and_safe_boundaries` | Content origin, fractional samples, same-cell adjacent pixels, padding, nonzero allocation, 2x DPI, geometry/scale changes, encoding switch, safe out-of-content/extreme handling, cell clamp regression |
| R4 wheel seam and output | `mouse_wheels_both_units_signs_order_modifiers_and_multiplicity`; widget `wheel_conversion_preserves_line_and_pixel_payloads`; app `should_preserve_both_wheel_axes_and_units_at_terminal_bridge` | Both axes survive both increment units; horizontal signs, deterministic dual-axis bytes, all-modifier wheel, zero input and unchanged magnitude-independent multiplicity |
| R5 streaming | `mouse_fragmented_mode_query_reset_and_cancellation_streams`, `mouse_partial_query_and_disable_do_not_complete_prematurely` | Every stream split and byte-at-a-time feed; partial enable/query/disable do not complete prematurely; cancellation, unknown modes and RIS recovery |
| R6 input precedence and capture | `mouse_tracking_fallback_cancel_and_encoding_overflow_preserve_vt_ownership`; existing `sgr_mouse_routes_cell_coordinates_button_state_and_vt_capture_to_pty`, `vt_mouse_release_consumes_pending_local_capture_after_alt_transition`, `legacy_tracking_reports_wheel_without_local_scroll_until_tracking_is_disabled` | Suppressed tracked input stays consumed, capture/release is retained, Cancel emits nothing and clears drag state, legacy silence replaced with exact fallback bytes |
| R6 compatibility | `mouse_encoding_flags_keep_existing_buffer_local_lifetimes`; existing `event_dispatch_preserves_local_escape_ime_focus_and_keyup_policy`, focus/IME/alternate-screen tests in the workspace | Existing mode lifetime, keyboard, focus, IME and local routing regressions remain enabled |

## Review and contract assessment

Inline review inspected the complete production/test/checklist diff, new test/evidence files, existing parser dispatch, widget/app wheel producers and screen buffer transition owners. No confirmed code blocker remains in that inspected scope. R1–R6 implementation and automated requirements have criterion-specific evidence above; mandatory native mouse interoperability remains **needs-verification**, so this record is not full spec acceptance.

Iteration findings were resolved: preserve local wheel routing for no-viewport/headless input, collapse the clippy-reported input branch, and correct the new lifetime test to distinguish retained `?47` re-entry from existing clear-on-entry `?1049` semantics. The latter was a test setup error, not a product regression; production buffer ownership was not changed.

## Quality checks

Final integrated check results are recorded below after all source/test edits. Ignored tests are not accepted by a passing workspace result. Compiler notes about Windows incremental-cache directory access were non-fatal; they did not fail clippy or tests.

| Command | Outcome |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — 2384 passed, 0 failed, 10 ignored across test binaries/doctests |
| `cargo test -p harbor-terminal --no-default-features --lib mouse` | PASS — 16 passed, 0 failed |
| `python scripts/check_docs.py` | PASS |
| `python scripts/checklist_summary.py` | PASS — 855 checked / 144 open / 999 total; inventory only, not runtime acceptance |

### Probe-only follow-up checks

The interactive probe and its usage guide were added after the Rust implementation gates above; no Rust production code changed in this follow-up. `python -B -m unittest discover -s scripts/tests -p 'test_verify_mouse_windows.py' -q` passes 24 portable protocol/logging/cleanup tests on Python 3.12.10; native console I/O is mocked. `python -B scripts/verify_mouse_windows.py --help`, `python scripts/check_docs.py`, `python scripts/checklist_summary.py`, and `git diff --check` also pass. These checks do not establish physical mouse or native application acceptance.

## Native Windows scenarios — NOT RUN

These are required application acceptance gaps, not automated passes. No application version or classic Console API/VT-input use was measured in an actual Harbor mouse session. The installed conhost version above must not be substituted for a session's transport evidence.

| Workload/scenario | Version and input API | Expected | Observed | Outcome |
| --- | --- | --- | --- | --- |
| Windows Vim legacy mouse workload: enable `1000` without extended encoding, click/drag/release | Vim version uncollected; classic Console API vs VT input unverified | At final cell (3,2), left press `1b 5b 4d 20 23 22`, release `1b 5b 4d 23 23 22`; eligible tracking motion and no local selection | No native session observed | NOT RUN |
| Windows byte-capture probe: UTF-8/urxvt/SGR encoding switch and independent queries | Probe name/version and VT-input setup must be recorded when executed | UTF-8 x=128 `1b 5b 4d 20 c2 a0 22`; urxvt `ESC[32;3;2M`; SGR `ESC[<0;3;2M`; independent set/reset replies and disable fallback | Recording-writer tests only; no application capture | NOT RUN |
| Windows byte-capture probe: SGR horizontal/dual-axis wheel in both units | Probe and device versions uncollected; VT-input mode unverified | Positive/negative dx give 66/67; vertical then horizontal, one per axis; no zero/zero bytes | No physical mouse/touchpad run | NOT RUN |
| Windows byte-capture probe: SGR pixel precision, allocation/DPI changes and local/VT switching | Probe, display/device and scale uncollected | Content origin (1,1), adjacent physical pixels distinguishable; correct scale/padding/allocation; disabling tracking restores local routing | Synthetic geometry only | NOT RUN |

### Runtime handoff

Use [`scripts/verify_mouse_windows.py`](../scripts/verify_mouse_windows.py) as the application-side VT byte probe; [run instructions and verdict limits](mouse-probe.md) describe mode menus, expected bytes and JSONL logs. Its Python/mock tests verify the probe, not a native Harbor mouse session. Creating the probe does not change the NOT RUN outcomes above.

Build and launch the current dirty-tree Harbor executable. Record exact build/profile, OS, app/probe version and whether it reads classic Console mouse records or VT-input bytes (`ENABLE_VIRTUAL_TERMINAL_INPUT` for a Windows byte probe). Exercise the named scenarios with application mode output through ConPTY, not direct calls into the encoder. Record raw expected/observed bytes, pointer position/geometry, physical device and scale, encoding switches and capture/local behavior. Distinguish line-wheel, pixel-wheel and dual-axis input; if the device cannot generate a required unit/axis, retain NOT RUN for that device path. Then append scoped PASS/FAIL evidence bound to that executable revision.

Excluded/unvalidated: press-only `?9`, highlight/DEC locator, extra buttons, physical devices not exercised, WSL/SSH/tmux transports and native Unix runtime. Existing unrelated native test passes do not establish these mouse scenarios. The spec remains unaccepted until its required native Windows evidence is available.
