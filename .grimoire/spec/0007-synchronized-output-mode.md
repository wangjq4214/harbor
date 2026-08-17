# Synchronized Output Mode

**Spec ID:** 0007
**Status:** In Progress
**Date:** 2026-08-17

## Requirement

Harbor must support DEC private synchronized-output mode (`CSI ?2026h/l`) so applications can batch terminal changes without intermediate presentation while recovery, reset, and session shutdown keep the visible terminal bounded and usable.

## Solution

Maintain synchronized-output state at the Terminal/session boundary as a saturating nesting counter: every `?2026h` increments it, every `?2026l` decrements it only when nonzero, and ordinary presentation is eligible only at zero. DECRQM reports Set for any nonzero count.

Parsing, screen mutation, damage tracking, GPU preparation, and terminal rendering continue while the count is nonzero; only ordinary frame presentation is deferred. The Terminal Widget Bridge exposes this presentation eligibility through its existing terminal-to-external-schedule path. The Widget Runtime Frame Scheduler owns a monotonic 100 ms recovery deadline and requests a normal runtime frame/present at each deadline while synchronization remains enabled. A matching final disable resumes ordinary presentation immediately and cancels the synchronized-output deadline.

RIS clears the counter as part of Terminal hard reset. PTY/session close also clears it and removes suppression for any surviving terminal view. DECSTR deliberately leaves the counter unchanged. The generic scheduling and presentation contracts must remain terminal-type-free inside `harbor-widget`; direct-host compatibility is preserved by keeping Terminal frame demand host-neutral rather than transferring window, surface, GPU, or PTY ownership.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Private-mode and query dispatch | `harbor-parser` → `harbor-terminal` Screen/Terminal | Existing CSI private-mode dispatch and platform-neutral reply buffering | `?2026` nested state transitions and DECRQM/DECRPM status |
| Presentation eligibility | `harbor-terminal` → Terminal Widget Bridge / `harbor-widget` scheduling | Terminal-owned suppression state and host-neutral frame demand | Deferred ordinary presents plus bounded recovery scheduling |
| Frame execution | `harbor-widget` Runtime Scheduler → winit presenter | Generic redraw/deadline effects and borrowed host frame resources | Normal forced presentation without Terminal acquiring or presenting a surface |
| Session cleanup | Terminal I/O lifecycle → Terminal/session state | RIS dispatch and PTY reader/session close signal | Cleared sync state and restored ordinary presentation eligibility |

## End-to-End Tests

### E2E: A completed synchronized batch presents once

- **Given:** A visible terminal with synchronized output disabled.
- **When:** PTY output enables `?2026`, changes visible screen content in multiple chunks, and disables `?2026` before 100 ms elapses.
- **Then:** The terminal model contains all changes, no intermediate terminal frame is presented, and the next frame presents the completed batch.

### E2E: Nested and mismatched boundaries cannot deadlock presentation

- **Given:** A visible terminal using ordinary presentation.
- **When:** PTY output enables synchronization twice, disables it once, emits more changes, sends the final disable, then sends additional disables.
- **Then:** Presentation remains deferred until the final matched disable, resumes after it, and extra disables neither underflow the state nor suppress later frames.

### E2E: An unclosed batch receives bounded recovery presents

- **Given:** Synchronized output is enabled and the terminal has dirty visible content.
- **When:** No matching disable arrives and successive monotonic 100 ms deadlines occur.
- **Then:** The Runtime uses its normal presentation path to present a current frame at each bounded recovery deadline while DECRQM continues to report Set.

### E2E: Reset and session termination release suppression

- **Given:** Synchronization is enabled with pending terminal content.
- **When:** The PTY sends RIS, or the PTY reader/session closes.
- **Then:** The nesting count is cleared and a surviving terminal view follows ordinary redraw policy; DECSTR alone leaves synchronization enabled.

## Decisions

### Saturating session-owned nesting

- **Choice:** Represent `?2026` with a saturated Terminal/session-owned counter; unmatched disables are no-ops and DECRQM reports Set while the count is nonzero.
- **Reason:** This provides nested semantics without exposing parser internals and prevents malformed output from underflowing state or permanently gating presentation.
- **ADR reference:** [0010-parser-minimal-public-api](../adr/0010-parser-minimal-public-api.md), [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md), [0017-platform-neutral-terminal-replies](../adr/0017-platform-neutral-terminal-replies.md), [0022-synchronized-output-presentation-recovery](../adr/0022-synchronized-output-presentation-recovery.md)

### Defer presentation, not terminal processing

- **Choice:** Continue terminal parsing, state mutation, damage tracking, preparation, and rendering work; defer only ordinary frame presentation.
- **Reason:** Terminal retains its model and renderer while the Runtime retains frame policy and `harbor-widget` stays independent of Terminal types and platform resources.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Runtime-owned bounded recovery using host-neutral demand

- **Choice:** The Widget Runtime Frame Scheduler owns the recurring 100 ms recovery deadline and forced frame request, consuming terminal-owned eligibility through the generic external-schedule contract; Terminal frame demand remains host-neutral for direct hosting.
- **Reason:** The Runtime already coalesces redraws and deadlines, while the host-neutral demand contract preserves the scheduling and ownership boundaries used by both widget-hosted and standalone Terminal paths.
- **ADR reference:** [0020-host-neutral-terminal-frame-scheduling](../adr/0020-host-neutral-terminal-frame-scheduling.md), [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md), [0022-synchronized-output-presentation-recovery](../adr/0022-synchronized-output-presentation-recovery.md)

### Narrow reset behavior

- **Choice:** RIS and PTY/session close clear synchronized output; DECSTR does not.
- **Reason:** Hard reset and I/O teardown must not leave a dead presentation gate, while preserving the existing distinction between hard reset, soft reset, and alternate-screen lifecycle.
- **ADR reference:** [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md), [0019-alternate-screen-buffer-isolation](../adr/0019-alternate-screen-buffer-isolation.md), [0022-synchronized-output-presentation-recovery](../adr/0022-synchronized-output-presentation-recovery.md)

### ADR compatibility cross-check

- **Choice:** Preserve all existing ownership, rendering, scheduling, parser, reply, and alternate-screen boundaries.
- **Reason:** ADRs 0001, 0004, 0006, 0008, and 0014 are superseded; ADRs 0002, 0003, 0007, 0009, 0016, and 0018 are unaffected; ADRs 0005, 0010–0013, 0015, 0017, and 0019–0022 are directly complied with. No conflict is introduced.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0010-parser-minimal-public-api](../adr/0010-parser-minimal-public-api.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md), [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md), [0022-synchronized-output-presentation-recovery](../adr/0022-synchronized-output-presentation-recovery.md)

## Test Plan

- **Integration tests:** Drive private CSI sequences through the streaming parser, Screen, and Terminal output path; verify DECRQM status, nesting, RIS, DECSTR, and PTY/session-close behavior. Use deterministic Runtime/Scheduler time and the presenter contract to verify deferred presentation, deadline registration, recurring 100 ms recovery, final-disable cancellation, and zero-sized/suspended-surface behavior.
- **Manual tests:** Exercise Vim, tmux, or another full-screen application that emits `?2026h/l`; verify a short redraw batch does not tear, and an intentionally omitted disable remains visually fresh within 100 ms. Run the Windows smoke coverage blocked by issue #93.
- **Performance thresholds:** Dirty synchronized terminal content must not be withheld from visible presentation for more than 100 ms; ordinary non-synchronized output retains current scheduling and presentation behavior.
- **Edge cases:** Repeated enables; disables at zero; disable after forced recovery; query during nesting; RIS and DECSTR during a batch; PTY EOF during a batch; alternate-screen transitions; concurrent blink/autoscroll deadlines; temporarily non-drawable surfaces.

## Out of Scope

- General DCS or non-`?2026` synchronized-output extensions.
- Deferring parser execution, Screen mutation, damage tracking, or GPU preparation.
- Renderer performance work, adaptive/configurable recovery timeouts, and a user-facing synchronization setting.
- Mouse/focus protocol changes, alternate-screen redesign, resize/reflow work, or wide-cell editing changes.
- Transferring Window, Surface, Device, Queue, or PTY ownership into Terminal.

## Future Evolution

- Re-evaluate the fixed 100 ms recovery interval using measured latency and throughput data.
- Add diagnostics if applications regularly rely on forced recovery.
- Generalize presentation eligibility only if future protocol modes require it, while preserving the generic external-schedule boundary.
- Expand conformance comparison against additional terminal emulators and use the result to refine Windows compatibility smoke coverage.
