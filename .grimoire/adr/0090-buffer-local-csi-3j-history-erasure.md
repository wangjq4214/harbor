# Buffer-local CSI 3 J history erasure

**Status:** Completed
**Date:** 2026-10-26
**Sources:** CSI 3 J refinement conversation; the user approved the recommendation that erasure affect only the active buffer and never saved primary history behind an alternate screen.
**Related:** [ADR 0019](./0019-alternate-screen-buffer-isolation.md) (whole-screen buffer isolation).

## Context

Before this change, Harbor dispatched ordinary `CSI Ps J` to screen erasure, but implemented only modes 0, 1 and 2. Adding the saved-lines erasure extension introduced an observable scope choice when the alternate screen is active: operate on the active buffer only, or reach through to the saved primary screen.

The existing whole-screen isolation contract keeps primary scrollback independent of alternate-screen edits. The approved choice preserves that boundary rather than giving this command cross-buffer reach.

## Decision

Scope ordinary `CSI 3 J` saved-lines erasure to the active buffer. Do not traverse into the saved primary screen or another inactive buffer. In particular, receiving it on the alternate screen must not erase primary history that will be restored on exit.

This scope preserves ADR 0019 rather than replacing it. The implementation does not reconstruct the screen or change configured history limits.

## Consequences

- A full-screen alternate application cannot use `CSI 3 J` to destroy the shell's saved primary history.
- Implementation and regression tests must preserve saved primary history across an alternate-screen `CSI 3 J` and the subsequent return to primary.
- Do not interpret this command as session-wide history erasure.

## Verification

Automated regressions in [history_tests.rs](../../crates/harbor-terminal/src/screen/history_tests.rs) cover `?47`, `?1047` and `?1049` alternate-screen erasure and primary restoration, along with history accounting, split CSI input, read evidence, selection and viewport behavior. [Search regressions](../../crates/harbor-terminal/src/literal_search/tests.rs) verify that erased history cannot remain an actionable match.

Both `cargo test -p harbor-terminal` and `cargo test -p harbor-terminal --no-default-features` passed on 2026-10-26, including all seven new CSI 3 J tests. Existing ignored tests were not enabled. Native interactive shell/tmux/SSH acceptance was not performed.
