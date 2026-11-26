# In-Window Liquid-Glass Confirmation Overlay

**Status:** Testing
**Date:** 2026-11-24
**Sources:** GitHub issue #63 refinement discussion (2026-11-24): owner approved migrating both multiline paste confirmation and OSC 52 clipboard-write confirmation into the main application window as native liquid-glass overlays participating in `PopupSlot`, preserving existing confirmation shortcuts, safe default focus, and per-request security invariants.
**Supersedes:** ADR 0007 (retired), ADR 0009 (retired); partially supersedes [ADR 0049](./0049-osc52-confirmation-window-and-foreground-policy.md) (independent confirmation window and secondary-window focus continuation exception) and [ADR 0077](./0077-extensible-exclusive-application-popups.md) (exclusion of paste/OSC 52 confirmation from the exclusive popup slot).
**Related:** [ADR 0057](./0057-widget-vector-icons-without-font-glyphs.md), [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0070](./0070-shared-application-glass-presets-with-local-overrides.md), [ADR 0076](./0076-native-liquid-glass-material-port.md), [ADR 0107](./0107-shared-palette-profile-flow-and-in-window-confirmation.md).

## Context

Harbor previously retained a separate borderless OS-level winit window (`ConfirmationWindow`) for multiline paste confirmation and later reused it for OSC 52 clipboard-write confirmation (retired ADRs 0007 and 0009, [ADR 0049](./0049-osc52-confirmation-window-and-foreground-policy.md)). Because that secondary window rendered without decorations, title, native liquid-glass backdrop integration, or application design-system styling, it appeared as an unstyled black rectangle detached from the main Harbor UI.

Harbor has since established a shared native liquid-glass design system ([ADR 0070](./0070-shared-application-glass-presets-with-local-overrides.md), [ADR 0076](./0076-native-liquid-glass-material-port.md), `docs/app/design-system.md`), built-in SDF vector icons ([ADR 0057](./0057-widget-vector-icons-without-font-glyphs.md)), and an extensible single-slot exclusive popup lifecycle (`PopupSlot`, [ADR 0077](./0077-extensible-exclusive-application-popups.md)).

Alternatives considered and rejected during issue #63 refinement:
- **Styled independent native window:** Preserves existing separate-window ADR boundaries, but cannot sample the main window's live terminal backdrop via `BackdropFilter` and splits modal presentation into two visual families alongside Quick Save ([ADR 0107](./0107-shared-palette-profile-flow-and-in-window-confirmation.md)).
- **Separate overlay layer stacked above `PopupSlot`:** Preserves a displaced Search or Command Palette underneath confirmation, but introduces a suspended-popup stack contrary to [ADR 0077](./0077-extensible-exclusive-application-popups.md)'s real-close replacement model.
- **Removing single-key `y`/`n` confirmation shortcuts:** Reduces accidental confirmation risk at the cost of breaking established muscle memory when safe default focus on Cancel/Deny already prevents blind Enter/Space approval.

## Decision

1. **Unified in-window liquid-glass confirmation overlay:** Replace the separate OS-level confirmation window with an in-window modal confirmation overlay inside the main application window for both multiline paste confirmation and OSC 52 clipboard-write confirmation, sharing Harbor's native liquid-glass appearance and design roles while retaining distinct copy, preview bounds, and effect paths for each flow.
2. **Participation in `PopupSlot` single-slot exclusivity:** Register confirmation overlays in the centralized extensible `PopupSlot` ([ADR 0077](./0077-extensible-exclusive-application-popups.md)) rather than excluding them. Opening confirmation closes any displaced participating popup (such as Search, Command Palette, or Quick Save) through its real close cleanup with no waiting queue or restoration stack. At most one confirmation request (paste or OSC 52) may be pending at a time; incoming paste or OSC 52 requests while a confirmation is active are rejected rather than queued or replacing previewed contents. Notifications remain excluded from `PopupSlot`.
3. **Shortcuts, safe default focus, and confirmation input gate:** Preserve confirmation shortcuts (`Escape`/`n`/`N` to cancel/deny, `y`/`Y` to confirm/allow, `ArrowUp`/`ArrowDown`/`PageUp`/`PageDown` and pointer wheel to scroll the preview) alongside standard `Tab`/`Shift+Tab` focus traversal and default initial focus on Cancel/Deny. While confirmation is active, workspace shortcuts are blocked and an application-owned confirmation input gate blocks terminal PTY keyboard input and new paste requests while allowing overlay interaction and continued terminal output rendering behind the live glass.
4. **Simplified OSC 52 strict main-window foreground policy:** Because OSC 52 confirmation now lives inside the main window, eliminate the secondary-window focus continuation exception from [ADR 0049](./0049-osc52-confirmation-window-and-foreground-policy.md). Strict main-window foreground eligibility applies continuously from request admission through confirmation resolution.
5. **Preserved per-request security invariants:** Retain `PaneBinding` plus weak `Terminal` revalidation, unchanged raw paste payload, approval-time `InputModes` encoding, and single Host-owned PTY write for paste; retain `PREVIEW_CHARS` truncation, `[clear clipboard]` empty handling, per-request authorization without permanent grants, source-closure/tab-change/external-focus cancellation, and content-free failure diagnostics for OSC 52.

## Consequences

- `ConfirmationWindow` and secondary-window event/surface routing become obsolete once both paste and OSC 52 confirmation migrate into the main window.
- Cross-window input gating (`ADR 0009`) is replaced by an in-window confirmation input gate coordinated with `PopupSlot` and `FocusScope::modal(true)`.
- OSC 52 foreground authorization becomes strictly main-window-only across both admission and confirmation without requiring cross-window focus continuity tracking.
- Opening a confirmation overlay cleanly closes any active Search or Command Palette without changing palette-initiated paste completion reporting.
- Note: Automated workspace, offscreen `dx12-warp` GPU, and ConPTY acceptance tests are implemented and verified; interactive Windows native desktop visual and real-clipboard acceptance remains pending sign-off (`Spec 0057` Verification Evidence).
