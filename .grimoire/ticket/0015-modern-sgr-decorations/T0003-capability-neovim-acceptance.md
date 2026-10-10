# Capability Discovery and Neovim Acceptance

**Ticket ID:** T0003
**Source:** [Spec 0018 R6-R7 and integrated R1-R5](../../spec/0018-modern-sgr-decorations.md), [ADR 0051](../../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md), [Validation](../../../docs/validation.md).
**Status:** Done

## Goal

Applications discover only implemented/evidenced styled underline support, and the complete modern SGR slice has reproducible Windows Neovim and synthetic VT acceptance without inflating terminal identity or parent-issue completion.

## Affected Surfaces

- `crates/harbor-terminal/src/parser/xtgettcap.rs`: existing boolean capability registry and exact bounded replies.
- Existing DECRQSS and integration tests for combined style/color/conceal/overline state.
- Terminal update/render integration and regression tests for combined features; defects return to the responsible producer without inventing new policy.
- Windows synthetic VT / Neovim procedures or scoped harness fixtures, self-contained validation summaries and affected protocol/status documentation.

## Approach

Consume the two completed state/render producers, their focused and scoped runtime evidence, and the approved spec. Add Su to the existing registry only after the styled underline producer's support is verified. Use the established XTGETTCAP transport and bounds, not a new registry architecture.

Verify the combined state and exact replies, then exercise Neovim undercurl/diagnostic-style highlighting and underline color in an actual Harbor Windows/ConPTY session. Record highlight configuration and versions so the scenario is reproducible; distinguish any explicit application setup from automatic detection. Synthetic tests supplement the application run and cover styles that the selected Neovim build does not emit.

Preserve `TERM=xterm-256color`, DA and unrelated capabilities. Produce precise feature-specific documentation/evidence, not a claim that all #162 outcomes or SSH/tmux paths were accepted.

## Dependencies and Coordination

- **Blocked by:** T0001 for implemented/evidenced styled/color underlines; T0002 for complete conceal/overline and combined acceptance.
- **Blocks:** No other ticket in this set.
- **Coordination risks:** Procedures and recording fixtures may be prepared earlier, but final assertions and capability claims must use the actual integrated revision. Shared serializers/constructors must not omit another producer's state.
- Run native fixtures in a controlled environment; do not access secrets or unrelated terminal/clipboard contents. A missing safe test environment is BLOCKED, not a pass.

## Acceptance

- [x] The verified styled underline implementation is discoverable as boolean Su through the current XTGETTCAP registry. The single reply is exactly `\x1bP1+r5375\x1b\\`; existing TN/RGB/u8, mixed/unknown queries, fragmentation, cancellation and request/reply bounds remain correct.
- [x] No TERM/DA/Kitty identity change, unsupported protocol advertisement, or new PTY reply writer is introduced.
- [x] Combined DECRQSS status represents style/color, conceal and overline, has exact-byte assertions and reconstructs the authoritative pen from a reset state. Legacy/default bytes and bounds remain intact.
- [x] Actual Windows Neovim shows configured colored undercurl/diagnostic-style highlighting; editing/redraw, resize/DPI observations, alternate-screen exit and subsequent shell output are recorded with expected/observed results.
- [x] The record includes exact Neovim/Windows/ConPTY versions, launch/transport path, reproducible highlight setup, binary revision/dirty scope, commands/steps and artifacts. No WSL/SSH/tmux or automatic-detection pass is inferred from a different path.
- [x] Controlled synthetic cases cover all styles, explicit spaces, wide cells, indexed/RGB colors, inverse, off/re-enable/59, overline, conceal and original copied text, hyperlink fallback, state lifetimes/reset, and malformed/fragmented recovery.
- [x] Integrated cases prove conceal suppresses modern colored/style underlines and hyperlink fallback without discarding source; decorated tails and styled erase/fill blanks survive primary reflow while ordinary tails retain their existing rule.
- [x] GPU geometry/encode and Windows visual evidence cover clipping and relevant invalidation/retained-damage paths; a skipped GPU or native case is explicitly NOT RUN/BLOCKED.
- [x] Applicable existing replies, OSC metadata, focus, SGR mouse, IME, synchronized output, selection/copy and alternate-screen regressions remain intact. This is change regression, not reopening completed prerequisite sub-issues.
- [x] Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py`; record unrun commands/reasons without claiming passes.
- [x] Durable evidence uses PASS/FAIL/NOT RUN/BLOCKED and names exclusions. Protocol/status documentation marks only implemented and evidenced rows; #162's remaining non-SGR scope stays explicit.

## Delivery evidence

[Scoped integrated acceptance](../../../docs/modern-sgr-acceptance.md) records the baseline/dirty source and binary SHA256, exact Su and combined status/replay tests, actual GPU readback, native Neovim 0.12.5 configured highlights/edit/redraw/resize/exit, synthetic all-style/color/conceal/overline/copy and pending-input resize observations. Final workspace: 2310 passed, 0 failed, 5 ignored; all required quality commands passed. Native 1.5x DPI was observed; cross-monitor transitions, native font reload/IME and WSL/SSH/tmux were not run. Neovim highlighting was explicitly configured, not an automatic-discovery claim. Done is the stated modern-SGR slice only; #162's remaining non-SGR work is not closed.

## Out of Scope

Changing TERM/terminfo deployment, new SSH/tmux contracts, Kitty identity or other Kitty protocols, retesting #173/#174 as prerequisites, adding palette/mouse/box-drawing outcomes, fabricating historical or new execution evidence, and closing #162 solely because this slice passed.
