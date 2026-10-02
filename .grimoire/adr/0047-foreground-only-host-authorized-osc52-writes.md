# Foreground-Only Host-Authorized OSC 52 Writes

**Status:** Superseded
**Date:** 2026-09-30
**Superseded by:** [ADR 0049](./0049-osc52-confirmation-window-and-foreground-policy.md)

## Context

Issue [#174](https://github.com/wangjq4214/harbor/issues/174) introduces bounded OSC 52 writes after the built-in typed-action refactor, while requiring host-owned clipboard effects and no writes from inactive or stale sessions. The refinement discussion compared default per-write confirmation with direct foreground writes: Windows Terminal and Alacritty check focus before applying clipboard writes, while Kitty and Ghostty default to allowing writes and gate reads separately.

## Decision

Default to allowing writes only from the current active tab while the Harbor main window has focus, with configurable allow, deny, and confirm behavior for eligible foreground requests; this preserves normal remote-editor copying without making every write a confirmation interaction. The terminal emits typed requests without system clipboard I/O, and the host checks live session identity and current eligibility before applying the existing clipboard effect; OSC 52 reads remain denied in this delivery.

## Consequences

- Inactive tabs, unfocused or minimized windows, and stale sessions must not change the clipboard, including when configuration is allow.
- Diagnostics and verification artifacts must not contain sensitive clipboard contents; the exact denial notification UX remains to be refined.
- Default per-write confirmation is rejected as unnecessarily disruptive for routine remote-editor copying; confirm remains an opt-in stricter policy.
- Request transport, permission decisions, and delayed confirmation must preserve the originating session identity rather than target whichever tab is active later.
- At the time of this permission decision, exact capacity and retention scope were unresolved. They were subsequently settled on 2026-09-30 in [ADR 0048](./0048-bounded-osc52-specific-clipboard-capacity.md); this supplements rather than changes the permission policy.

## Sources

- Settled user discussion: acceptance of the foreground-allow/background-deny/read-deny recommendation, followed by a request for a larger buffer (2026-09-30).
- [Windows Terminal focus check](https://github.com/microsoft/terminal/blob/main/src/cascadia/TerminalCore/TerminalApi.cpp)
- [Alacritty host clipboard focus check](https://github.com/alacritty/alacritty/blob/master/alacritty/src/event.rs)
- [Kitty clipboard policy](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.clipboard_control)
- [Ghostty clipboard policy](https://github.com/ghostty-org/ghostty/blob/main/src/config/Config.zig)
- `.grimoire/CONTEXT-terminal-protocol.md`: OSC 52 Host-Authorized Clipboard Write.
