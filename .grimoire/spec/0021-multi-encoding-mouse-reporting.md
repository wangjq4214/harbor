# Multi-Encoding Mouse Reporting

**Spec ID:** 0021
**Status:** Implemented; native Windows acceptance pending
**Date:** 2026-10-11
**Sources:** [Issue #192](https://github.com/wangjq4214/harbor/issues/192), the user's 2026-10-11 approval of the recommended contract and Spec-only endpoint, [ADR 0055](../adr/0055-multi-encoding-mouse-reporting.md), [Terminal Protocol Context](../CONTEXT-terminal-protocol.md#vt-mouse-reporting), [Protocol Checklist](../../docs/protocol/checklist.md#28-mouse-protocol-output), and [Validation Policy](../../docs/validation.md).

## Scope and Settled Contract

Applications using legacy, UTF-8, urxvt, SGR cell or SGR pixel mouse encoding must receive well-formed reports through the Harbor terminal input/PTY path. Add horizontal SGR wheel reports without regressing existing SGR press/release/motion/vertical-wheel behavior.

The approved scope includes `?1005`, `?1015` and `?1016`. Preserve independent tracking priority `?1003 > ?1002 > ?1000`; independently retain encoding enablement with priority `?1016 > ?1006 > ?1015 > ?1005 > legacy`. Legacy encoding preserves the effective tracking mode's event semantics. Press-only `?9` is not included.

This resolves the issue's ambiguous “X10 … press-only” wording: `CSI M` is the legacy wire format, not a requirement to suppress releases/motion under `1000/1002/1003`. The issue's checklist reference to section 31.2 is not mouse coverage: that section currently concerns Home/End. Relevant acceptance surfaces are 17.6, 22 and 28.1-28.4; the `?9` and press-only X10 items remain unclaimed.


## Requirements

### R1. Tracking and encoding are independent

- Preserve independent enablement and per-mode query state for `1000`, `1002` and `1003`. Effective tracking is the highest enabled mode; clearing it reveals the next enabled mode.
- `1000` reports button press/release and wheel input, but not motion. `1002` additionally reports motion while a button is held. `1003` additionally reports motion without a held button. Preserve the existing motion/reporting behavior rather than introducing unrelated motion deduplication.
- Preserve independent enablement and per-mode query state for `1005`, `1006`, `1015` and `1016`. Enabling an encoding does not clear other encoding flags. Selection is deterministic, independent of enable-command order, using the approved priority above.
- Encoding enablement alone never activates tracking. With no tracking mode enabled, no mouse reports are emitted and existing local selection/scrollback routing remains available.
- With tracking active and all extended encodings disabled, use legacy encoding instead of the current silent SGR-only behavior. Clearing an encoding switches to the next enabled encoding, ultimately legacy; it does not disable tracking.
- Complete pending PTY mode sequences must be reflected consistently in event filtering, coordinate-unit selection and encoding before the event is reported. An event must not be mapped using stale cell/pixel state and then encoded using newly parsed mode state.

### R2. Exact wire formats and event codes

Use the existing 7-bit `CSI` prefix (`ESC [`). The notation below describes bytes, not literal spaces between fields. `x` and `y` are final 1-based column/row or pixel coordinates; `b` is the protocol event code before any legacy offset.

| Encoding | Required wire representation |
| --- | --- |
| Legacy fallback | `ESC [ M` followed by single bytes `b + 32`, `x + 32`, `y + 32` |
| UTF-8 (`1005`) | `ESC [ M` followed by UTF-8 encodings of values `b + 32`, `x + 32`, `y + 32`; do not UTF-8 encode the entire legacy byte vector as if it were already text |
| urxvt (`1015`) | `ESC [` followed by decimal `b + 32;x;yM`, without `<`; coordinates do not receive the legacy offset |
| SGR cell (`1006`) | Existing decimal `ESC [ < b;x;yM` for press/motion/wheel, and `ESC [ < b;x;ym` for release |
| SGR pixel (`1016`) | Same event-code and final-character rules as `1006`, using pixel coordinates instead of cells |

- Preserve button codes left = 0, middle = 1, right = 2; modifier bits Shift = 4, Alt or Meta = 8, Ctrl = 16. Alt and Meta share one bit, not two additive bits. Preserve the motion bit 32 and the existing held-button preparation for drag events.
- Legacy, UTF-8 and urxvt release uses the release code 3 plus modifiers, with no SGR lowercase final character or retained released-button identity. SGR cell/pixel release preserves the released-button identity and uses lowercase `m`.
- Wheel reports use uppercase `M`, not synthetic button-up events. Preserve vertical codes 64/65 and existing report multiplicity.
- Legacy supports final 1-based coordinates 1-223 inclusive. If either coordinate exceeds 223, suppress that complete report; do not wrap, truncate or clamp an unrepresentable coordinate into a different legacy position. This encoding limit is separate from the existing viewport-to-cell clamping.
- UTF-8 mode follows xterm's maximum encodable coordinate 2015 and encodes offset numeric values as UTF-8, including the button field. Cover the one-byte/two-byte boundary and maximum coordinate with byte-level tests. Out-of-range handling must be explicit and tested; no malformed UTF-8, numeric wrap or unsupported larger-coordinate support claim is permitted. This contract does not select a new UTF-8 overflow policy beyond well-formed, documented handling.
- Decimal encodings do not inherit the legacy 223-coordinate limit. Coordinates and offsets must not cause arithmetic overflow, panic, truncated reports or numeric wrap.

Wire-format authority: [Xterm Control Sequences — Mouse Tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), checked through the [xterm 401 source mirror](https://github.com/SvitloLinux-pkgs/xterm/blob/301a7a5d00abf646b6e116e17629c15bb67887cd/ctlseqs.txt). This protocol reference does not override Harbor's approved independent-state priorities or its existing SGR event behavior.

### R3. Coordinate units and geometry ownership

- Preserve the existing SGR cell conversion and viewport clamping. Cell reports remain relative to terminal content, not the whole host surface or scrollback history.
- `1016` is itself an SGR pixel encoding selector; it does not require `1006` also to be enabled. It wins when both are enabled. Clearing `1016` reveals `1006` if still enabled, otherwise the next enabled encoding.
- Terminal derives pixel coordinates from routed logical position and current physical geometry: apply input scale, subtract terminal allocation origin and content padding, retain sub-cell precision, and report integer physical pixels with a 1-based content origin. Do not derive pixels by multiplying an already rounded cell coordinate by cell size.
- The content's top-left pixel reports `(1,1)`. For an in-content nonnegative position, the zero-based physical pixel index is the floor of its content-relative position; add one exactly once when forming the protocol coordinates.
- Nonzero terminal allocation origin, nonzero padding, fractional positions and non-1x DPI must not add host offsets, double-apply scale or shift the origin. Geometry/DPI changes must affect subsequent samples without a stale unit conversion.
- `WheelLine`/`WheelPixel` describe wheel increment units independently of cell/pixel coordinate selection. Either wheel phase can report cell coordinates under `1006` or pixel coordinates under `1016`.
- Tests must distinguish viewport boundaries from encoding limits and exercise out-of-content and extreme coordinate samples. Preserve existing cell clamping; new pixel boundary handling must be well-defined, documented and safe, without claiming another position through numeric wrap. This contract does not prescribe a new upper-edge pixel clamp/drop policy.

### R4. Horizontal and dual-axis wheel input

- Preserve both `dx` and `dy` through the existing winit/widget/app/terminal seam for both wheel units. Do not infer horizontal movement from `dy` or require a new native event type.
- With SGR cell or pixel encoding active, nonzero horizontal movement produces protocol button 66 for left and 67 for right, with the same modifier bits and uppercase `M` as other wheel reports.
- Match input sign to the installed winit event convention, not a guess about natural-scrolling gestures. Winit defines positive horizontal delta as content movement to the right, revealing the left; the protocol left report is therefore used for positive `dx`, and right for negative `dx`. Keep the existing `dy > 0 => 64`, `dy < 0 => 65` mapping.
- A pure horizontal event must not disappear just because `dy` is zero. A zero/zero event emits no report.
- A dual-axis event emits one report for each nonzero axis, in a fixed deterministic order. The particular order is an implementation detail, but must be documented and asserted as exact bytes; dropping either axis or varying order is not acceptable.
- Do not add magnitude-based repetition, cross-event wheel accumulation or a new vertical-wheel quantization policy. Preserve existing local scrollback behavior when tracking is disabled.

Direction references: [winit MouseScrollDelta](https://docs.rs/winit/latest/winit/event/enum.MouseScrollDelta.html) and the left/right event-code definitions in [xterm.js Types](https://github.com/xtermjs/xterm.js/blob/c58ea3637f3968e0e6e79cd92cf9aace7ef89ee2/src/common/Types.ts) and [MouseStateService](https://github.com/xtermjs/xterm.js/blob/c58ea3637f3968e0e6e79cd92cf9aace7ef89ee2/src/common/services/MouseStateService.ts). Only the direction/code mapping is referenced; do not import those implementations' tracking, coordinate or wheel-accumulation policies.

### R5. Mode queries, parser behavior and reset

- Recognize DECSET/DECRST `1005`, `1015` and `1016`, both individually and in existing multi-mode sequences. Repeated enable/disable remains idempotent.
- `CSI ? Ps $ p` returns `CSI ? Ps;1 $ y` for an enabled supported mode and `CSI ? Ps;2 $ y` for a disabled supported mode, including lower-priority modes that are enabled but not currently selected.
- Query each newly supported mode before enable, after enable, while another encoding wins, and after disable/RIS. Unsupported modes, including `?9` in this scope, remain consume-ignored and return the existing unknown response `CSI ? Ps;0 $ y`.
- RIS clears all tracking and encoding flags, preserves existing pointer/capture cleanup obligations and stops mouse reports until tracking is enabled again.
- Existing streaming-parser handling applies: partial mode/query sequences produce no premature completed action; completed whole versus fragmented sequences have equivalent state and replies. Cover splits within mode numbers, markers/intermediates and final bytes, plus recovery after cancellation and unknown modes.
- Do not add mouse-report decoding on the PTY output side. The output parser recognizes mode/query commands; pointer reports are generated on the input side.

### R6. Interaction and compatibility protection

- Tracked input continues to take precedence over local selection, scrollbar interaction and scrollback. Suppression by tracking filters or an encoding limit must not turn the event into an unintended local action.
- Preserve host-facing pointer capture/release outcomes. `Cancel` sends no mouse bytes but clears held-button state and releases capture as applicable; a subsequent no-button move must not be treated as a stale drag.
- Preserve existing SGR press/release/motion/modifier/vertical-wheel bytes, keyboard, focus, IME and alternate-screen behavior through applicable regression tests. Do not redefine mouse mode lifetimes across buffer transitions as part of this change.
- Replace the existing SGR-only silent-consumption assertions with legacy-fallback assertions. Keep the remaining assertions as regression protection rather than weakening byte comparisons.

## End-to-End Tests

Feed application mode commands through terminal output parsing, deliver pointer samples through Terminal, and assert application-visible bytes using the recording PTY writer. Byte strings below use Rust escape notation. Geometry examples have sufficient rows/columns to keep the samples inside terminal content.

| Case | Given / input | Observable outcome |
| --- | --- | --- |
| Legacy default press/release | Enable `1000` only; left Down/Up at final cell `(3,2)` | Exact `b"\x1b[M\x20\x23\x22"` then `b"\x1b[M\x23\x23\x22"`; local selection does not receive the press |
| UTF-8 offset encoding | Enable `1000;1005`; left Down at final cell `(128,2)` | Exact `b"\x1b[M\x20\xc2\xa0\x22"`; at x = 95/96 verify `\x7f` / `\xc2\x80`; at x = 2015 verify `\xdf\xbf` |
| urxvt press/release | Enable `1000;1015`; left Down/Up at final cell `(3,2)` | Exact `b"\x1b[32;3;2M"` then `b"\x1b[35;3;2M"`, with no `<` and no coordinate offset |
| Existing SGR baseline | Enable `1000;1006`; left Down/Up at final cell `(3,2)` | Exact `b"\x1b[<0;3;2M"` then `b"\x1b[<0;3;2m"` |
| Pixel precision and switch | Scale 1, origin `(0,0)`, padding 2, cell size `(10,20)`; logical sample `(27,23)`; enable `1000;1016`, then also enable `1006`, then clear `1016` | First two left presses report `b"\x1b[<0;26;22M"`; after clearing `1016`, same sample reports `b"\x1b[<0;3;2M"`; `1016` works without `1006` |
| Physical offset and DPI | Scale 2, physical allocation origin `(100,50)`, padding 4; logical sample `(64,39)` under `1000;1016` | Physical content position `(24,24)` reports exact `b"\x1b[<0;25;25M"`; adjacent physical pixels in the same cell remain distinguishable |
| Horizontal wheel | Enable `1000;1006`; final cell `(3,2)`; `dx = +1/-1`, `dy = 0`, in both wheel phases | Exact `b"\x1b[<66;3;2M"` / `b"\x1b[<67;3;2M"`; equivalent pixel-mode cases use pixel coordinates |
| Dual-axis and zero wheel | SGR tracking; both deltas nonzero, then both zero; include modifiers and larger/fractional delta magnitudes | One report per nonzero axis in documented fixed order, no extra repetitions, no zero/zero output; vertical-only bytes remain unchanged |
| Tracking fallback and motion | Enable `1000;1002;1003`, press/drag/release/move, then clear `1003`, then `1002`; repeat for each encoding | Effective tracking falls back as specified; no-button motion and held-button drag are distinguished; each encoding uses its own release format |
| Encoding fallback and queries | Enable all encoding flags in different orders with tracking active; clear `1016`, `1006`, `1015`, `1005` successively and query all flags | Output switches pixel → SGR cell → urxvt → UTF-8 → legacy; queries expose each independent flag rather than only the winner |
| Modifiers | Each encoding, all supported buttons and Shift/Alt/Meta/Ctrl combinations, press/release/motion/wheel where applicable | Exact event bits and offsets; Alt+Meta contributes bit 8 only once; SGR released-button identity is preserved |
| Encoding limits versus viewport | Large in-bounds grid, legacy x/y = 223 and 224, UTF-8 transition/max/beyond max, decimal x/y above 223; negative and extreme raw samples | Legacy 223 encodes as `\xff`, 224 drops the entire report without local fallback; other formats meet their own documented range behavior; viewport clamps do not hide the boundary under test |
| Cancel and reset | Capture a tracked press, Cancel, subsequent no-button move; then RIS and queries; also enable only encodings with tracking off | No Cancel bytes or stale drag; correct capture release; flags reset and no reports with tracking off; existing local behavior remains available |
| Fragmented commands and queued mode changes | Whole versus split DECSET/DECRST/DECRQM/RIS streams; complete queued encoding change immediately before a pointer event | Equivalent completed state/reply bytes; no premature report activation; coordinate units match the final selected encoding |
| Windows application path | Named/versioned legacy workload plus SGR horizontal/pixel workload through actual Harbor/ConPTY, using a byte-capture application/probe where needed | Expected versus observed bytes, mode switching and local/VT interaction are recorded per scenario, with PASS/FAIL/NOT RUN and explicit transport/device exclusions |

Existing integration-test anchors include `sgr_mouse_routes_cell_coordinates_button_state_and_vt_capture_to_pty`, `terminal_mouse_filters_follow_effective_mode_fallback_and_cancel_state`, `queued_output_updates_modes_before_input_encoding`, and widget `wheel_conversion_preserves_line_and_pixel_payloads`. Add focused encoder/parser/mode tests as needed; a synthetic writer alone does not prove the native runtime path.

## Verification and Definition of Done

The [scoped evidence record](../../docs/multi-encoding-mouse-acceptance.md) identifies implemented encoding policies and automated tests. Native Windows legacy application and byte-probe switching, horizontal/dual-axis and pixel/DPI/local-routing scenarios remain NOT RUN. Record actual application/probe, device, OS/ConPTY versions, classic Console API versus VT-input use, geometry, expected/observed bytes and PASS/FAIL/NOT RUN/BLOCKED per scenario; an unavailable device path stays unvalidated. Run the [standard quality gates](../../docs/validation.md#standard-quality-gates). Checklist 17.6/22/28.1–28.4 may claim only evidenced scope, never press-only `?9` or unrelated Home/End work.

## Out of Scope

- Press-only `?9`, highlight tracking, DEC locator, additional physical mouse buttons or unrelated private-mode support.
- PTY-output-side mouse-report decoding.
- Reworking existing SGR press/release/motion/vertical-wheel semantics, motion deduplication, wheel accumulation or magnitude-based repetition.
- Redesigning selection/scrollback/capture ownership, buffer-transition mode lifetimes, native input transport or terminal identity/capability advertisement.
- Claims for unexercised Windows applications, input devices, WSL/SSH/tmux transports or native Unix runtime support.
