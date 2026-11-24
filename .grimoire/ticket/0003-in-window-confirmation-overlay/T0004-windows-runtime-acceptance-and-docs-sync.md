# Windows Runtime Acceptance and Documentation Sync

**Ticket ID:** T0004
**Source:** [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) (`R1`–`R7`, `E1`–`E9`, Test Plan), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md).
**Status:** Todo

## Goal

Execute the interactive Windows native visual and runtime acceptance suite for both in-window confirmation flows and synchronize repository documentation, verification evidence, and ADR status.

## Affected Surfaces

- Runtime verification on Windows build of `harbor`.
- Documentation: [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) (acceptance evidence), `docs/app/design-system.md` (if consumer notes are updated), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md) status progression, and cross-reference checks against [Spec 0017](../../spec/0017-host-authorized-osc52-clipboard-writes.md) and [Spec 0033](../../spec/0033-exclusive-extensible-application-popups.md).

## Approach

- Run a fresh Windows build across realistic terminal backdrops (quiet and busy/high-contrast output continuing behind the glass) and multiple DPI/window sizes (including constrained narrow/short viewports and effects-disabled fallback).
- Exercise end-to-end scenarios with real OS clipboard and live PTY shells: multiline paste approve/cancel, bracketed paste encoding, `Tab` focus traversal from default Cancel/Deny, `Esc`/`n`/`y`/arrow/`PageUp`/`PageDown`, scrollbar drag and wheel scrolling over wrapped long lines with logical line numbers, Search/Command Palette displacement, OSC 52 `confirm` allow/deny/truncation/empty-clear/external-focus-loss, and PTY input gating.
- Record revision, OS/DPI/backdrop conditions, exact commands/steps, expected vs observed results, and explicit `PASS` / `FAIL` / `NOT RUN` / `BLOCKED` entries without including sensitive clipboard payloads; update [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md) status as warranted by verified completion and run `python scripts/docs/check_docs.py`.

## Dependencies and Coordination

- **Blocked by:** T0003 (requires both paste and OSC 52 confirmation migrated in-window and `ConfirmationWindow` retired).
- **Blocks:** None.
- **Coordination risks:** Do not infer native visual or DPI acceptance solely from offscreen `dx12-warp` tests; keep any unrun scenarios explicitly marked `NOT RUN`.

## Acceptance

- [ ] Windows native runtime evidence covers multiline paste confirmation, OSC 52 `confirm` mode, scrollbar drag/wheel/keyboard scroll, logical line-number wrapping, safe default Cancel/Deny focus, live glass legibility over busy terminal output, constrained viewport behavior, multi-DPI rendering, and effects-disabled fallback.
- [ ] All automated workspace checks (`test_cargo.ps1`, `dx12-warp` offscreen GPU checks, `clippy`, `fmt`, `python scripts/docs/check_docs.py`, `git diff --check`) pass on the final revision.
- [ ] [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) and [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md) reflect truthful implementation/acceptance status with valid links and zero policy contradictions.

## Out of Scope

- Unrequested redesign of unrelated Harbor popups or adding features excluded by [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md).
