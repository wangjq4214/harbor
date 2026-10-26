# Synchronized Output Mode

**Spec ID:** 0007
**Status:** Accepted (owner-confirmed)
**Date:** 2026-08-17

## Requirement

Support DEC private synchronized output (`CSI ?2026h/l`) without intermediate presentation or unbounded visible freezes.

## Solution

- Terminal/session owns a saturating nesting counter: enable increments, disable decrements only above zero, ordinary presentation is eligible only at zero. DECRQM reports Set at every nonzero depth.
- Parsing, screen mutation, damage tracking, GPU preparation and rendering continue during synchronization. Only ordinary presentation is deferred.
- The Terminal Widget Bridge exposes eligibility through host-neutral external scheduling. The Runtime Frame Scheduler owns a monotonic recurring 100 ms recovery deadline and forces normal runtime presentation while synchronization remains enabled. Final disable immediately releases presentation and cancels recovery.
- RIS and PTY EOF/session close clear all nesting and release any surviving view. DECSTR preserves nesting and query state. Cleanup must propagate through the matched bridge and cancel stale recovery demand without a busy wake, including after a forced recovery.
- Widget scheduling remains terminal-type-free. Terminal does not acquire Window, Surface, Device, Queue or PTY ownership from the host.

[ADR 0022](../adr/0022-synchronized-output-presentation-recovery.md) records nesting, presentation recovery and lifecycle policy; [ADR 0020](../adr/0020-host-neutral-terminal-frame-scheduling.md) and [ADR 0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md) preserve host-neutral demand.

## End-to-End Tests

A short batch spanning multiple PTY chunks presents its completed content once after disable. Nested enables remain suppressed until fully matched; extra disables cannot underflow. Recovery repeatedly presents dirty content within 100 ms without clearing DECRQM Set. RIS and session close restore ordinary presentation; DECSTR does not.

### E2E: An unclosed batch receives bounded recovery presents

An omitted disable produces successive monotonic 100 ms recovery presents through the normal Runtime path. Recovery is canceled after final disable, RIS or session close, without stale scheduling.

## Test Plan

Drive streaming private CSI through parser, Screen, reply buffering, Terminal and bridge. Deterministic scheduler/presenter tests cover nesting/saturation, mismatched disables, DECRQM bytes, recurring recovery, cancellation, RIS/DECSTR/EOF after recovery, alternate-screen transitions, concurrent blink/autoscroll, and non-drawable/suspended surfaces.

**Historical acceptance qualification (0007/T0004):** Prior implementation/test source did not establish Windows smoke or performance results; issue #93 was identified as the smoke blocker. Owner acceptance does not assert those checks ran or that the blocker was fixed. Ordinary non-synchronized scheduling must remain unchanged. Exercise Vim/tmux or another emitting application and an omitted-disable case on Windows.

Historical 0007/T0001–T0003 contracts survive in Solution and the recovery case above.

## Out of Scope

Other DCS/synchronization families, deferred parsing/model/GPU preparation, configurable recovery, renderer optimization, mouse/focus changes, alternate-screen redesign, resize/reflow, and wide-cell editing.
