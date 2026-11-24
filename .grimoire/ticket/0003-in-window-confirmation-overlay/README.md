# In-Window Liquid-Glass Confirmation Overlay — Issue #63

**Source:** [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md), [GitHub #63](https://github.com/wangjq4214/harbor/issues/63).
**Ticket folder:** `.grimoire/ticket/0003-in-window-confirmation-overlay/`

## Overview

Migrate both multiline paste confirmation and OSC 52 clipboard-write confirmation from the unstyled separate OS window (`ConfirmationWindow`) into a shared in-window native liquid-glass overlay participating in `PopupSlot`, aligned with Harbor's design system (`docs/app/design-system.md`), built-in SDF warning icon, inset bordered preview with alternating row tint and logical line numbers, `ScrollView` scrollbar, preserved confirmation shortcuts, safe default Cancel/Deny focus, and strict per-request security invariants.

## Permissions and Knowledge Boundary

This ticket set was produced under an authorized `/grimoire-refine` session alongside [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md), updated [CONTEXT.md](../../CONTEXT.md) / [CONTEXT-terminal-protocol.md](../../CONTEXT-terminal-protocol.md), and [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md). Ticket creation does not itself authorize code implementation or Git commits; invoke `/grimoire-loop` separately when ready to execute.

Consume only the settled spec and cited ADRs without inventing new requirements, heuristics (dangerous-command regex highlighting remains out of scope), or unapproved visual rules.

## Delivery Surfaces

- Generic widget icons and controls: `crates/harbor-widget/src/icon.rs`, `crates/harbor-widget/assets/lucide/`, widget quad SDF shader and tests.
- Application confirmation UI and popup lifecycle: `crates/harbor-app/src/confirmation.rs`, `crates/harbor-app/src/confirmation/preview.rs`, `crates/harbor-app/src/appearance/design_system.rs`, `crates/harbor-app/src/popups.rs`, `crates/harbor-app/src/command.rs`, `crates/harbor-app/src/workspace.rs`.
- Host coordination, input gate, and window retirement: `src/dialog.rs`, `src/clipboard.rs`, `src/shell.rs`, and their unit/integration tests.
- Documentation and acceptance evidence: [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md), `docs/app/design-system.md`, [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md).

## Dependencies

| Producer | Consumer | Why consumer cannot proceed first |
| --- | --- | --- |
| T0001 | T0002 | Host paste migration requires the built-in warning icon, redesigned `Popup`-backed confirmation overlay, generation-scoped commands, and logical-line-numbered `ScrollView` preview. |
| T0002 | T0003 | Retiring `ConfirmationWindow` and unifying single-confirmation exclusion across paste and OSC 52 cannot complete safely until paste confirmation no longer depends on the separate window. |
| T0003 | T0004 | Full Windows native runtime acceptance and documentation reconciliation require both paste and OSC 52 flows running in-window with the old window retired. |

The dependency graph is strictly linear and acyclic: `T0001 -> T0002 -> T0003 -> T0004`.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002, T0003 | Shared edits in `crates/harbor-app/src/confirmation.rs`, `crates/harbor-app/src/command.rs`, and `src/shell.rs` | Keep `ConfirmationWindow` compilable during T0001/T0002 until T0003 migrates OSC 52 and removes `ConfirmationWindow`; sequence edits along `T0001 -> T0002 -> T0003`. |
| T0002, T0003 | Mutual exclusion and input gate drift between paste and OSC 52 | Share one confirmation-gate predicate and one active confirmation admission check in `ActiveSession` / `WorkspaceUiController` rather than maintaining duplicate modal flags. |
| T0001, T0004 | Offscreen GPU tests passing while native desktop readability or DPI clipping regresses | Treat T0001 `dx12-warp` GPU checks as scoped offscreen verification only; record native Windows observations explicitly in T0004. |

## Recommended Order

1. **T0001** — Build the bundled warning vector icon and redesigned in-window liquid-glass confirmation overlay UI (`Popup` + `ScrollView` + logical line numbers + shortcuts + default Cancel/Deny focus) with mounted and offscreen GPU tests.
2. **T0002** — Migrate multiline paste confirmation into `PopupSlot`, enforce the in-window confirmation input gate, and preserve target/mode revalidation and palette completion reporting.
3. **T0003** — Migrate OSC 52 clipboard-write confirmation into the shared `PopupSlot` overlay under strict main-window foreground policy, enforce paste/OSC 52 mutual exclusion, and retire `ConfirmationWindow`.
4. **T0004** — Execute the Windows native runtime acceptance matrix and synchronize documentation and ADR status.

## Requirement Coverage

| Spec scope | Responsible tickets |
| --- | --- |
| R1 Unified liquid-glass overlay, shrink-wrapped scroll layout | T0001 (UI/GPU), T0002 (paste), T0003 (OSC 52), T0004 (native) |
| R2 Warning SDF icon, footer alignment, states, safe default focus | T0001, T0004 |
| R3 Inset border, alternating row tint, logical line numbers, scrollbar, escaping/truncation | T0001 (UI/scroll/line numbers), T0003 (OSC 52 markers), T0004 |
| R4 `PopupSlot` exclusivity, palette handoff, single pending confirmation, generation safety | T0001 (Popup/generation), T0002 (paste/palette), T0003 (OSC 52 exclusion) |
| R5 Shortcuts, `blocks_shortcuts()`, input gate, focus restoration | T0001 (overlay shortcuts/focus), T0002 (input gate & PTY block), T0003 (OSC 52 gate) |
| R6 Paste and OSC 52 per-request security invariants | T0002 (paste), T0003 (OSC 52 strict foreground), T0004 (native verification) |
| R7 `harbor-widget` boundary and `ConfirmationWindow` retirement | T0001 (icon boundary), T0003 (`ConfirmationWindow` removal) |
| E1–E9 End-to-end verification scenarios | T0001 (`E2`, `E8`, `E9`), T0002 (`E1`, `E4`, `E6`, `E7`), T0003 (`E3`, `E5`, `E6`), T0004 (full native E2E) |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-warning-icon-and-confirmation-overlay-ui.md](./T0001-warning-icon-and-confirmation-overlay-ui.md) | Bundled SDF warning icon and redesigned liquid-glass confirmation overlay component & `Popup` |
| T0002 | [T0002-in-window-paste-confirmation-and-input-gate.md](./T0002-in-window-paste-confirmation-and-input-gate.md) | In-window multiline paste confirmation via `PopupSlot` and confirmation PTY input gate |
| T0003 | [T0003-in-window-osc52-confirmation-and-window-retirement.md](./T0003-in-window-osc52-confirmation-and-window-retirement.md) | In-window OSC 52 confirmation, paste/OSC 52 mutual exclusion, and `ConfirmationWindow` retirement |
| T0004 | [T0004-windows-runtime-acceptance-and-docs-sync.md](./T0004-windows-runtime-acceptance-and-docs-sync.md) | Windows native runtime acceptance evidence and documentation/ADR synchronization |
