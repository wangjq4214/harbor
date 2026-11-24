# Warning Icon and Liquid-Glass Confirmation Overlay UI

**Ticket ID:** T0001
**Source:** [Spec 0057](../../spec/0057-in-window-confirmation-overlay.md) (`R1`, `R2`, `R3`, `R4`, `R5`, `R7`, `E2`, `E8`, `E9`), [ADR 0111](../../adr/0111-in-window-liquid-glass-confirmation-overlay.md), [Harbor Design System](../../../docs/app/design-system.md).
**Status:** Todo

## Goal

Deliver the bundled vector warning icon in `harbor-widget` and the redesigned in-window liquid-glass confirmation overlay UI in `harbor-app` (implementing `Popup` for `PopupSlot`, `ThemeProvider`, fixed header/footer, inset bordered scrollable preview with alternating row tint, logical line numbers, `ScrollView` scrollbar, preserved shortcuts, and default focus on Cancel/Deny).

## Affected Surfaces

- `crates/harbor-widget`: `src/icon.rs`, `assets/lucide/` (plus `assets/lucide/LICENSE` compliance), quad SDF shader / icon renderer, and widget icon tests.
- `crates/harbor-app`: `src/confirmation.rs`, `src/confirmation/preview.rs`, `src/appearance/design_system.rs`, `src/command.rs`, `src/workspace.rs` (overlay rendering/action wiring), and mounted/GPU tests.

## Approach

- Add a bundled Lucide warning icon (e.g., `LucideIcon::TriangleAlert`) following [ADR 0057](../../adr/0057-widget-vector-icons-without-font-glyphs.md) with zero Harbor business dependencies in `harbor-widget` ([ADR 0064](../../adr/0064-widget-and-application-component-ownership.md)).
- Build the confirmation overlay using `ThemeProvider` (`design_system::theme`) and `GlassAppearance::command_palette().panel(...)` inside `Portal` / `ConstrainedBox` / `FocusScope::modal(true)` following the fixed-header, shrink-wrapped `ScrollView` body, right-aligned footer pattern used by Quick Save.
- Pair `Icon::new(...)` (`design_system::WARNING`) with `TextLabel` (`TextRole::Title`) in the fixed header; right-align `Cancel`/`Deny` (`Quiet`/`Secondary`, assigned initial `FocusHandle`) on the left and `Paste`/`Allow` (`Primary`) on the right with `Row::wrap(true)` for narrow windows.
- Upgrade `PreviewPane` (or equivalent preview view inside `ScrollView`) to render inside an inset `DecoratedBox` border, apply subtle alternating row tinting, display a left gutter with 1-based logical line numbers (leaving continuation wrapped rows blank without incrementing the counter), and support `ScrollView` scrollbar drag, pointer wheel, and `ArrowUp`/`ArrowDown`/`PageUp`/`PageDown`.
- Register generation-scoped confirmation commands/callbacks (`Popup::blocks_shortcuts() == true`) and handle `Escape`/`n`/`N` (cancel/deny) and `y`/`Y` (confirm/allow) while keeping `Tab`/`Shift+Tab` and `Enter`/`Space` button semantics intact. Keep legacy `ConfirmationWindow` callers compiling until T0002/T0003 complete migration.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0002 (Host paste confirmation needs the `Popup`-backed overlay and generation-scoped actions), T0003, T0004.
- **Coordination risks:** Shared edits in `crates/harbor-app/src/confirmation.rs` and `crates/harbor-app/src/command.rs`; preserve existing `build_confirmation_root` / `build_clipboard_confirmation_root` compatibility shims or update existing callers cleanly so T0002/T0003 integrate without breaking intermediate builds.

## Acceptance

- [ ] `harbor-widget` builds and passes icon tests for the new warning `LucideIcon` variant with zero references to Harbor paste, OSC 52, or `PopupSlot` types.
- [ ] Mounted `harbor-app` tests verify both Paste (`Cancel` / `Paste`) and OSC 52 (`Deny this request` / `Allow this request`) overlays: default initial focus lands on Cancel/Deny; `Enter`/`Space` initially triggers Cancel/Deny rather than Paste/Allow; `y`/`Y` confirms; `Escape`/`n`/`N` cancels; `Tab` moves focus to Paste/Allow; stale-generation actions are ignored.
- [ ] Preview unit and mounted tests verify escaping (`safe_preview_line`), inset border, alternating row tint primitives, `ScrollView` scrollability (`ArrowUp`/`ArrowDown`/`PageUp`/`PageDown` and wheel), and logical line numbers across wrapped long lines (continuation rows have blank gutter cells and do not increment logical line numbers).
- [ ] Layout tests under constrained viewports (e.g., narrow/short sizes) confirm pinned header/footer visibility, wrapped non-overlapping buttons, and unclipped preview text.
- [ ] `HARBOR_TEST_GPU=dx12-warp` offscreen GPU test renders both Paste and OSC 52 overlays across DPI scales `1.0` / `1.5` / `2.0` showing crisp warning icon, glass backdrop sampling, inset preview border, alternating row tint, scrollbar, and focus ring.

## Out of Scope

- Wiring Host `PasteController` or OSC 52 `ClipboardPolicy` into `PopupSlot` (T0002, T0003).
- Retiring `ConfirmationWindow` in `src/dialog.rs` (T0003).
- Dangerous-command regex highlighting or new TOML appearance settings.
