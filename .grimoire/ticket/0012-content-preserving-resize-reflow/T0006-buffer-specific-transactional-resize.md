# Buffer-Specific Transactional Resize

**Ticket ID:** T0006
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #170
**Status:** Done

## Goal

Integrate primary reflow, rectangular alternate-screen resize, and synchronous PTY resize as one prepared transaction that either commits complete consistent geometry or leaves the terminal model unchanged.

## Affected Surfaces

- **Screen resize ownership:** Prepared active Screen state, saved primary, parked alternate, cursor/margins, tab stops, damage, and hyperlink cleanup.
- **Alternate-screen policy:** Top-left rectangular resize, no text reflow or scrollback, wide-boundary repair, and cursor clamp.
- **Terminal/PTY boundary:** Normalize dimensions, prepare model, resize PTY, discard on failure, and infallibly swap on success.
- **Pointer/selection integration:** Preserve canonical selection through successful resize and remove unconditional `pointer.clear()` behavior.
- **Failure and mode tests:** Preparation failure, PTY failure, retry, active/parked alternate, saved-primary restoration, and `?47`/`?1047`/`?1048`/`?1049` coverage.

## Approach

Define a prepared Screen/Terminal resize result that owns every allocation and mapping needed for commit. Prepare normal buffers, buffer-specific cursor state, margins, tab stops, damage, anchor projections, and hyperlink reachability before calling `PtyControl::resize`. On PTY failure, drop the prepared value and retain old model/projections; on success, commit only by ownership swap with no allocation or fallible recomputation.

Reflow primary state through T0005. Resize active and parked alternate Screens as rectangular top-left surfaces, repair wide boundaries, clamp their cursor state, and prohibit alternate scrollback. While alternate mode is active, reflow the saved primary independently so exit restores current geometry. Normalize model and PTY width to at least two and rows to at least one at the Terminal boundary.

## Dependencies and Coordination

- **Blocked by:** T0005 — transaction preparation must consume the complete primary width/height/capacity result.
- **Blocks:** T0007 — runtime evidence requires the integrated application resize path and failure semantics.
- **Coordination risks:** `Terminal::try_resize_if_changed`, `Screen::resize`, pointer ownership, `harbor-pty` control tests, and alternate-screen tests. Keep reflow policy in the terminal model and PTY control limited to applying normalized geometry.

## Acceptance

- [ ] Model preparation performs all allocation and validation without mutating the live Terminal or PTY.
- [ ] Preparation failure leaves PTY, model geometry, cursor/review/selection projections, and active buffer unchanged.
- [ ] PTY failure discards prepared state, preserves old model state, and permits a later successful retry.
- [ ] Successful PTY resize is followed by an allocation-free, infallible model ownership swap.
- [ ] Terminal, model, and PTY observe the same normalized minimum geometry of two columns and one row.
- [ ] Active and parked alternate screens preserve rectangular top-left content, have no reflow scrollback, repair wide edges, and clamp cursor state.
- [ ] A saved primary is independently reflowed while alternate mode is active and restores correct content, cursor, review, and valid selection on exit.
- [ ] Resize restores full-height scroll region, clamps horizontal margins, extends tab stops every eight columns, marks the full viewport dirty, and safely cleans unreachable hyperlink IDs.
- [ ] Successful resize preserves selection through anchor projection instead of unconditionally clearing pointer state.
- [ ] Focused `?47`, `?1047`, `?1048`, `?1049`, repeated resize, failure, and retry tests pass.

## Out of Scope

- Windows runtime execution and final performance measurements.
- Changing alternate-screen mode ownership from ADR-0019.
- Asynchronous PTY I/O or a new process-control architecture.
