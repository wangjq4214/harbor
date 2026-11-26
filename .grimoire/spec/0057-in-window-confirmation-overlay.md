# In-Window Liquid-Glass Confirmation Overlay

**Spec ID:** 0057
**Status:** In Progress
**Date:** 2026-11-24
**Issue:** [#63](https://github.com/wangjq4214/harbor/issues/63)
**Sources:**
- GitHub issue [#63](https://github.com/wangjq4214/harbor/issues/63) and refinement decisions (2026-11-24)
- [ADR 0111](../adr/0111-in-window-liquid-glass-confirmation-overlay.md) (superseding retired ADRs 0007 and 0009, partially superseding [ADR 0049](../adr/0049-osc52-confirmation-window-and-foreground-policy.md) and [ADR 0077](../adr/0077-extensible-exclusive-application-popups.md))
- [Harbor Design System](../../docs/app/design-system.md) and [Spec 0054](./0054-harbor-design-system-and-quick-save.md)
- [Spec 0017](./0017-host-authorized-osc52-clipboard-writes.md) and [Spec 0033](./0033-exclusive-extensible-application-popups.md)

## Requirements

### R1 — Unified In-Window Liquid-Glass Confirmation Overlay

Both multiline paste confirmation and OSC 52 clipboard-write confirmation render as modal overlays inside the main Harbor window using Harbor's shared native liquid-glass panel appearance and application design-system theme, rather than spawning a separate OS-level winit window.

- Reuse the shared liquid-glass surface family (`GlassAppearance::command_palette()`) and application design roles (`design_system::theme`) via `ThemeProvider`, without a full-window dark scrim, fixed background image, or independent optical pipeline.
- Maintain a fixed header area, a shrink-wrapped scrollable preview body, and a fixed footer area inside bounded viewport margins so short previews size compactly and long previews scroll the body while keeping the header and action buttons visible and reachable across constrained window sizes and DPI scales.
- Terminal output and live glass backdrop transmission continue operating behind the overlay.

### R2 — Visual Hierarchy, Warning Icon, and Action Footer

- The fixed header pairs a built-in vector warning icon rendered via Harbor's bundled SDF vector-icon pipeline ([ADR 0057](../adr/0057-widget-vector-icons-without-font-glyphs.md)) and tinted with the application warning role (`design_system::WARNING`) with the confirmation title (paste line count for paste confirmation; source tab and byte size for OSC 52 confirmation).
- The fixed footer right-aligns the two actions when space permits and wraps cleanly in narrow viewports without overlap: the deny/cancel action (`Cancel` for paste, `Deny this request` for OSC 52) on the left with subordinate styling (`Quiet` or `Secondary`), and the confirm/allow action (`Paste` for paste, `Allow this request` for OSC 52) on the right as the sole solid primary emphasis button (`Primary`).
- All interactive controls expose distinct normal, hover, pressed, disabled, and keyboard-focus-ring states independent of pointer hover.
- Initial keyboard focus lands safely on the cancel/deny control so accidental `Enter` or `Space` keypresses never approve a paste or clipboard write.

### R3 — Inset Scrollable Preview with Alternating Row Tint and Logical Line Numbers

- Present the escaped preview inside a visibly bounded, inset rounded surface with subtle alternating row tinting to separate the preview area from the outer glass header and footer.
- Display logical line numbers in a left gutter aligned with each logical source line: when a long logical line wraps across multiple visual rows, the first visual row shows its 1-based logical line number and continuation visual rows leave the line-number gutter blank without incrementing the logical line counter.
- Embed the preview inside `ScrollView` so it displays a semi-transparent draggable scrollbar when content overflows the viewport and supports pointer-wheel, scrollbar-drag, and keyboard scrolling (`ArrowUp`, `ArrowDown`, `PageUp`, `PageDown`).
- Preserve unchanged raw payload semantics: preview escaping (`safe_preview_line`) and wrapping remain presentation-only and never alter the confirmed paste text or OSC 52 clipboard payload. Retain OSC 52's bounded `PREVIEW_CHARS` (1024-character) truncation marker (`[preview truncated]`) and `[clear clipboard]` empty-payload marker.

### R4 — Single-Slot Exclusive Popup Participation

- Confirmation overlays participate in `PopupSlot` ([ADR 0077](../adr/0077-extensible-exclusive-application-popups.md), [Spec 0033](./0033-exclusive-extensible-application-popups.md)) using the existing extensible `Popup` trait without adding a central popup-kind enum or pairwise exclusion logic.
- Opening a confirmation overlay closes any active displaced participating popup (Search, Command Palette, Quick Save, or another participating popup) through its real close cleanup, with no popup stack, queue, or automatic restoration upon confirmation close.
- Invoking Paste from the Command Palette cleanly transitions from the palette to the paste confirmation overlay and preserves `CommandCompletion::PendingConfirmation` plus final `Applied`/`Cancelled`/`Rejected` completion reporting.
- At most one confirmation request (either paste or OSC 52) may be pending at a time. While a confirmation overlay is active, incoming paste requests and incoming OSC 52 confirmation requests are rejected rather than queued, stacked, or replacing the previewed content.
- Generation-scoped lifetime checks ensure stale UI actions, clicks, or editor/IME deliveries from a retired popup generation cannot approve, cancel, modify, or close a replacement popup.
- Floating notifications remain excluded from `PopupSlot` and coexist without losing focus or three-second expiry ([ADR 0071](../adr/0071-three-second-notifications-including-errors.md)).

### R5 — Shortcuts, Input Gate, and Focus Restoration

- Preserve confirmation keyboard shortcuts while the overlay is active: `Escape`, `n`, and `N` cancel/deny; `y` and `Y` confirm/allow; `ArrowUp`, `ArrowDown`, `PageUp`, and `PageDown` scroll the preview; `Tab` and `Shift+Tab` traverse controls inside the modal `FocusScope`; `Enter` or `Space` activates the currently focused button.
- While confirmation is active, `blocks_shortcuts()` suppresses workspace keybindings, and an application-owned confirmation input gate blocks terminal PTY keyboard input and new paste requests while permitting overlay controls, wheel scrolling, and terminal output rendering.
- Dismissing or completing confirmation restores terminal focus according to existing popup-close focus restoration policy and releases the confirmation input gate.

### R6 — Preserved Per-Request Security Invariants

- **Paste confirmation:** Capture and revalidate `PaneBinding` plus weak `Terminal` identity at approval; cancel if the active tab/pane changes or the source session retires; encode the unchanged raw payload using the `Terminal`'s `InputModes` read at confirmation time; execute at most one Host-owned PTY write; enforce shared-GPU fault guarding so a fault observed at approval never produces a PTY write.
- **OSC 52 confirmation:** Enforce strict main-window foreground eligibility continuously from admission through approval (no secondary-window focus continuation exception); keep per-request `Allow this request` / `Deny this request` without permanent grants; cancel if the source session closes, the active tab/pane changes, or the main Harbor window loses foreground/becomes minimized; revalidate source and eligibility immediately before executing the Host clipboard effect; emit only content-free failure diagnostics.

### R7 — Ownership Boundaries and Window Retirement

- Preserve [ADR 0064](../adr/0064-widget-and-application-component-ownership.md): `harbor-widget` owns only generic UI/icon primitives (`LucideIcon`, `ScrollView`, `Button`, `DecoratedBox`, `FocusScope`, `Portal`) and remains free of Harbor paste, OSC 52, or `PopupSlot` business types.
- Retire `ConfirmationWindow` and obsolete secondary-window event/surface routing once both paste and OSC 52 confirmation run inside the main window.

## Solution

The confirmation UI in `crates/harbor-app/src/confirmation.rs` and `crates/harbor-app/src/confirmation/preview.rs` is rebuilt as an application component wrapped in `ThemeProvider` (`design_system::theme`) and `GlassAppearance::command_palette().panel(...)` mounted through a `Popup`-implementing confirmation state inside `PopupSlot`. A bundled Lucide warning icon is added to `crates/harbor-widget/src/icon.rs` and the widget quad SDF shader ([ADR 0057](../adr/0057-widget-vector-icons-without-font-glyphs.md)) for the header.

Preview lines are prepared with explicit logical-line metadata (`(Option<usize>, String)` or equivalent per-visual-row logical line number mapping) so wrapped continuation rows render without a line number while preserving `safe_preview_line` escaping and full vertical scrolling inside `ScrollView`. `PasteController` and OSC 52 confirmation coordination in `src/dialog.rs`, `src/clipboard.rs`, and `src/shell.rs` transition from managing an auxiliary `ConfirmationWindow` to opening, revalidating, and closing the `PopupSlot` confirmation overlay while enforcing the in-window confirmation input gate.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Vector icon and controls | `harbor-widget` (`icon.rs`, `quad` shader, `ScrollView`, `Button`, `FocusScope`) -> `harbor-app` | Validated styles, finite colors/dimensions, `ScrollController` | Sharp SDF warning icon, focus-ring/hover/press button states, draggable scrollbar, modal focus isolation |
| Confirmation overlay UI | `harbor-app::confirmation` + `appearance::design_system` -> `PopupSlot` / `Portal` | Title, action labels, escaped wrapped lines with logical line numbers, generation-scoped dispatchers | Liquid-glass modal panel, safe default Cancel/Deny focus, shortcut/scroll handling, generational action requests |
| Exclusive popup lifecycle | `PopupSlot` (`popups.rs`, `workspace.rs`) -> active popups & host commands | `Popup` trait implementation (`build`, `focus`, `close`, `blocks_shortcuts`) | Single active popup guarantee, real-close displacement of Search/Palette/Quick Save, non-blocking notifications |
| Paste & OSC 52 host coordination | `src/shell.rs`, `src/dialog.rs`, `src/clipboard.rs` -> `Terminal` & native clipboard | Captured `PaneBinding`/weak `Terminal` or `SessionClipboardWrite`, strict main-window foreground state, GPU fault state | Input gating, mutual exclusion across paste/OSC 52, source/foreground revalidation, single PTY write or Host clipboard effect |

## End-to-End Tests

### E1 — Multiline Paste Approval, Cancellation, Default Focus, and Encoding
- **Given:** A live terminal with bracketed paste enabled and no active popup, and multiline clipboard text containing newlines and control characters.
- **When:** Paste is triggered, opening the in-window confirmation overlay; inspect initial focus and test `Cancel` (button click / `Escape` / `n`), then reopen and test `Paste` (button click / `y` / `Tab` + `Enter`).
- **Then:** Initial focus starts on `Cancel`; cancelling writes zero bytes to the PTY, clears the input gate, and restores terminal focus; confirming performs exactly one bracketed-paste write of the unchanged raw payload using approval-time `InputModes`.

### E2 — Preview Visual Boundaries, Logical Line Numbers, and Scrollbar
- **Given:** Confirmation text containing both short lines and a long line wrapping across multiple visual rows, exceeding the visible preview viewport height.
- **When:** The preview renders and the user scrolls via `ScrollView` scrollbar drag, pointer wheel, and `ArrowUp`/`ArrowDown`/`PageUp`/`PageDown`.
- **Then:** The preview container renders inside a distinct inset rounded border with alternating row tinting and a visible scrollbar thumb; wrapped continuation rows keep a blank gutter cell and do not increment logical line numbers; header and footer remain pinned while the preview body scrolls.

### E3 — OSC 52 Confirmation, Truncation, Empty Clear, and Foreground Loss
- **Given:** OSC 52 policy set to `confirm` and main window focused on the active live tab.
- **When:** A terminal emits an OSC 52 write (>1024 chars) or an empty clear request, then the user either denies, allows, or switches focus to another application before deciding.
- **Then:** Preview shows `[preview truncated]` or `[clear clipboard]` with source tab and byte count; `Deny this request` / `Escape` leaves the OS clipboard untouched; `Allow this request` executes the exact payload once without creating a permanent grant; external focus loss cancels the pending overlay and makes no clipboard write.

### E4 — `PopupSlot` Exclusivity and Stale Generation Isolation
- **Given:** Search (with active query and highlights) or Command Palette is open.
- **When:** Paste confirmation or OSC 52 confirmation opens, and stale actions from the displaced popup's generation arrive afterward.
- **Then:** The displaced popup closes cleanly (clearing search query/highlights), confirmation becomes the sole active popup with focus on Cancel/Deny, palette-invoked Paste reports `PendingConfirmation` and later `Applied`/`Cancelled`, and stale-generation deliveries do not modify or close the confirmation overlay.

### E5 — Mutual Exclusion Between Paste and OSC 52 Confirmations
- **Given:** Either a paste confirmation overlay or an OSC 52 confirmation overlay is currently open.
- **When:** A new paste command or a new OSC 52 write request arrives.
- **Then:** The incoming request is rejected without queuing, opening a second overlay, or altering the currently previewed payload.

### E6 — Target Switch, Source Retirement, and GPU Fault Guard
- **Given:** A confirmation overlay is open for a specific `PaneBinding` / `SessionId`.
- **When:** The active pane/tab changes, the source shell closes, or a shared-GPU fault is observed before/at approval.
- **Then:** The confirmation overlay cancels cleanly, releases the input gate, and performs no PTY write or clipboard write.

### E7 — Confirmation Input Gate and Live Background Terminal Rendering
- **Given:** An in-window confirmation overlay is active over a live terminal emitting output.
- **When:** Keyboard and pointer events occur on the main window, including workspace shortcuts and clicks outside the modal card.
- **Then:** Terminal PTY input, workspace shortcuts, and terminal pointer activations are blocked while overlay buttons/scroll/shortcuts operate normally and new terminal output continues rendering behind the live glass.

### E8 — Constrained Viewport, Multi-DPI Scaling, and Live Glass Rendering
- **Given:** Constrained window sizes (e.g., narrow/short viewports) and multiple DPI scales (1.0, 1.5, 2.0) over quiet and busy/high-contrast terminal content.
- **When:** Mounted layout checks and `HARBOR_TEST_GPU=dx12-warp` offscreen GPU rendering run on both paste and OSC 52 overlays.
- **Then:** Header, footer, warning icon, inset preview border, alternating row tint, scrollbar, and focus rings render sharply without overlapping controls, clipped text starts, or full-window dark scrims.

### E9 — Widget Crate Boundary Verification
- **Given:** `crates/harbor-widget` with the new warning `LucideIcon` variant.
- **When:** Inspecting crate dependencies and widget tests.
- **Then:** `harbor-widget` contains zero references to paste, OSC 52, `PopupSlot`, or Harbor application state.

## Decisions

### Settled Architectural and Behavioral Decisions
- **In-window liquid-glass overlay over `PopupSlot`:** Confirmed by owner (2026-11-24); recorded in [ADR 0111](../adr/0111-in-window-liquid-glass-confirmation-overlay.md).
- **Shared UI for Paste and OSC 52 confirmation:** Confirmed by owner (2026-11-24); both flows share the unified overlay and preview component while preserving independent copy and security contracts.
- **Logical line numbers without dangerous-command regex highlighting:** Confirmed by owner (2026-11-24); wrapped continuation rows do not increment line numbers, and heuristic command highlighting is excluded.
- **Full shortcut preservation + Cancel/Deny default focus:** Confirmed by owner (2026-11-24); retains `Esc`/`n`/`N`/`y`/`Y`/arrows/`PageUp`/`PageDown` while defaulting focus to Cancel/Deny.

### Explicit Reversible Tuning Assumptions (Design-System Defaults)
- **Glass and theme recipes:** Reuse `GlassAppearance::command_palette()` and `design_system::theme` (`ThemeProvider`), with `Quiet` or `Secondary` styling on Cancel/Deny and `Primary` glacier-blue styling on Paste/Allow, and `design_system::WARNING` tint on the new vector warning icon (`LucideIcon::TriangleAlert` or equivalent).
- **Visual tuning:** Exact preview border alpha, alternating row-tint alpha, gutter spacing, panel max width/height bounds, and padding follow `docs/app/design-system.md` foundations and remain reversible tuning values rather than fixed acceptance thresholds.

## Test Plan

- **Automated unit & mounted widget/app tests (`cargo test` via `scripts/dev/test_cargo.ps1`):**
  - `harbor-widget`: icon variant SVG/SDF rendering mode coverage and validation.
  - `harbor-app`: mounted confirmation overlay tests covering Paste and OSC 52 variants, logical line-number wrapping, `ScrollView` scrolling, default Cancel/Deny focus, `Tab` traversal, `y`/`n`/`Esc`/arrow/`PageUp`/`PageDown` handling, generation isolation, constrained viewport layout, and `PopupSlot` replacement (Search, Command Palette).
  - Binary crate (`src/`): `PasteController` and `ClipboardPolicy` / `ActiveSession` tests covering input gate, palette completion reporting, mutual exclusion, strict main-window foreground check, approval-time `InputModes`, target retirement/tab switch, GPU fault guard, and absence of `ConfirmationWindow` remnants.
- **Offscreen GPU verification (`HARBOR_TEST_GPU=dx12-warp`):**
  - Render paste and OSC 52 confirmation overlays across scales `1.0` / `1.5` / `2.0` over synthetic backgrounds to verify warning icon rasterization, glass panel, inset preview border, alternating row tint, scrollbar, and focus ring.
- **Windows native runtime acceptance:**
  - Interactive manual run on Windows verifying real clipboard multiline paste, real OSC 52 confirm/deny/external-focus-cancel, scrollbar drag, mouse wheel, DPI changes, busy terminal output behind live glass, and effects-disabled fallback. Record revision, OS/DPI, expected vs observed results, and explicit `PASS`/`FAIL`/`NOT RUN` statuses.

## Out of Scope

- Heuristic or regex-based dangerous-command detection/highlighting (e.g., `rm -rf`, `curl | sh`).
- Non-modal inline warning bars (Windows Terminal style) or permanent OSC 52 allow grants.
- New TOML configuration knobs for dialog colors/opacity/layout or coupling dialog styling to terminal color themes.
- Redesigning Search, Command Palette, Quick Save, or NotificationCard behavior/geometry.

## Verification Evidence

- **Branch & commits:** `feat/in-window-confirmation-overlay` (`a2b0638` production implementation across `T0001`–`T0003`, `3cd3a72` ConPTY fixture update; non-test production code unchanged between `a2b0638` and `3cd3a72`)
- **Environment:** Windows `10.0.26300.0`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`

### Automated Verification (`PASS`)

- Binary build (`a2b0638`): `pwsh -NoLogo -NoProfile -File scripts/dev/run_harbor.ps1 -BuildOnly` -> `PASS` (`D:\Code\harbor\target\debug\harbor.exe`)
- Workspace tests excluding `harbor-widget-macros` (`a2b0638`): `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked --workspace --exclude harbor-widget-macros` -> `PASS` (`harbor` 118 passed / 0 failed, `harbor-app` 253 passed / 0 failed, `harbor-terminal` 1194 passed + integration suites / 0 failed, `harbor-config` 25+3+4+3+3 passed / 0 failed, `harbor-parser` 8+17 passed / 0 failed, `harbor-pty` 19 passed / 0 failed, `harbor-widget` and `harbor-text` all passed / 0 failed)
- Offscreen `dx12-warp` workspace GPU suite (`a2b0638`): `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked --workspace gpu:: -- --ignored --test-threads=1` (`HARBOR_TEST_GPU=dx12-warp`) -> `PASS` (`harbor` 12 passed + `composable_button_rendering` 1 passed, `harbor-app` 9 passed, `harbor-terminal` 54 passed, `harbor-widget` unit 20 passed + `quad_renderer` 65 passed + `text_renderer` 4 passed; 0 failed)
- Confirmation-focused `dx12-warp` GPU tests (`3cd3a72`):
  - `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor-widget --test quad_renderer gpu::icons -- --ignored --test-threads=1` -> `PASS` (4 passed, 0 failed)
  - `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor-app confirmation_overlay_pixels -- --ignored --test-threads=1` -> `PASS` (1 passed, 0 failed)
- ConPTY suite (`3cd3a72`): `pwsh -NoLogo -NoProfile -File scripts/dev/test_pty.ps1` -> `PASS` (`harbor` 19 passed / 0 failed, `harbor-pty` 3 passed / 0 failed, `harbor-terminal` 3+16 passed / 0 failed)
- `harbor` unit & architecture suite (`3cd3a72`): `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor` -> `PASS` (118 passed / 0 failed)
- `no-default-features` check (`3cd3a72`): `pwsh -NoLogo -NoProfile -File scripts/dev/test_cargo.ps1 --locked -p harbor-terminal --no-default-features` -> `PASS` (877 unit + integration suites + doctests / 0 failed)
- Workspace lint, formatting, docs, and whitespace (`3cd3a72`):
  - `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` -> `PASS` (exit 0; pre-existing `harbor-pty` unused `parking_lot` manifest warning only)
  - `cargo fmt --all -- --check` -> `PASS`
  - `python scripts/docs/check_docs.py` -> `PASS` (233 files)
  - `git diff --check` -> `PASS`
- Pre-existing baseline failure outside scope (`a2b0638`, untouched): `harbor-widget-macros` `view_macro_ui` (`trybuild` `.stderr` snapshot mismatch: 8/51 failed on `rustc 1.99.1`, 20/51 failed on `+1.100.0`; `harbor-widget-macros` unit (10) and `renamed_dependency` (1) passed)

### Interactive Windows Native Runtime & Real-Clipboard Sign-Off (`NOT RUN`)

- Interactive desktop visual inspection over quiet and busy live terminal backdrops (`NOT RUN`)
- Multi-DPI and constrained viewport resizing with live native window chrome and effects-disabled fallback (`NOT RUN`)
- Real OS clipboard multiline paste approve/cancel, bracketed mode encoding, `Tab` focus traversal from default Cancel/Deny, scrollbar drag/wheel/keyboard scroll, Search/Palette displacement, and PTY gating (`NOT RUN`)
- Real OSC 52 `confirm` mode allow/deny, `[preview truncated]` / `[clear clipboard]`, and external-application focus-loss cancellation on desktop without modifying real OS clipboard contents outside explicit user action (`NOT RUN`)
