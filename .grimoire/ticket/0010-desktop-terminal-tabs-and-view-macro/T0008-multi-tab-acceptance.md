# Multi-Tab Resize, Scheduling, and Acceptance

**Ticket ID:** T0008
**Source:** [Spec: 0010-desktop-terminal-tabs-and-view-macro](../../spec/0010-desktop-terminal-tabs-and-view-macro.md)
**Status:** Todo

## Goal

The complete multi-tab slice has executable evidence for all-session resize, active-only input/render scheduling, deterministic cleanup, macro equivalence, Windows desktop behavior, and standard quality gates.

## Layers

- [ ] **Widget/Runtime:** Verify integrated layout, keying, interaction, scroll, focus, theme, macro, and post-layout effect contracts.
- [ ] **Terminal/PTY:** Broadcast changed active content TerminalSize to every tab and prove inactive/closed session behavior.
- [ ] **Runtime Host:** Complete event arbitration, final-tab window exit, paste modal gate, surface lifecycle, and failure handling.
- [ ] **Renderer/Scheduler:** Prove hidden tabs do not draw or keep Poll active and active external registrations change cleanly.
- [ ] **Verification:** Run focused suites, workspace gates, Windows smoke matrix, and performance/resource evidence.

## Approach

1. Convert each distinct valid terminal content allocation to a clamped TerminalSize and broadcast it once to all live tabs after layout.
2. Exercise active/inactive output, cursor blink, synchronized output, activation, close, surface recovery, minimize/restore, and DPI movement.
3. Add deterministic shutdown/leak tests for reader, PTY control, bridge callback, and external schedule cleanup.
4. Validate macro-authored UI behavior against handwritten fixtures and compile-fail diagnostics.
5. Run the repository validation policy and record Windows runtime evidence for input, window, and rendering behavior.

## Blocked by

- T0005 — Supplies allocation observation.
- T0006 — Supplies all tab resources and events.
- T0007 — Supplies integrated product UI.

## Blocks

- (none)

## Acceptance

- [ ] Every live tab receives each changed rows/columns value once; inactive tabs do not remain at stale window geometry.
- [ ] Zero size/minimize emits no invalid PTY resize, and restore/maximize/DPI transitions recover correctly.
- [ ] Only the active terminal receives input, paints, and contributes visible cursor scheduling.
- [ ] Sustained inactive output is ingested without hidden terminal draws or cursor-driven continuous Poll.
- [ ] Activation replaces external draw/schedule registration before presentation; close removes it with no stale callback.
- [ ] Paste confirmation blocks terminal and tab-changing commands until resolved.
- [ ] Final-tab close shuts down the session and closes Harbor.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and applicable checklist tooling pass.
- [ ] Windows smoke covers shell/Vim/less/output in multiple tabs, shortcuts, pointer selection, scrollback, Acrylic, resize, 100/150/200% DPI, minimize/restore, and surface recovery.
- [ ] Quiet-runtime and active-frame measurements remain within existing validation/performance policy; no leak or work proportional to hidden terminal cell count is introduced.

## Out of Scope

- Deferred feature additions from the source spec; failures discovered here become focused follow-up tickets rather than silent acceptance exceptions.
