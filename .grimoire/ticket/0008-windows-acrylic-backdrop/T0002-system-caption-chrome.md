# System caption chrome

**Ticket ID:** T0002
**Source:** [Spec: 0008-windows-acrylic-backdrop](../../spec/0008-windows-acrylic-backdrop.md)
**Status:** Todo

## Goal

The main-window caption strip shows Acrylic without painted title or icon, while DWM still draws working minimize, maximize, and close buttons and the taskbar still says Harbor.

## Layers

- [ ] **Config:** None — caption policy is Host/DWM, not a config constant.
- [ ] **Terminal:** None — cell paint is unchanged.
- [ ] **Winit Runtime Integration:** None — caption is non-client chrome, not a presented frame.
- [ ] **Runtime Host:** Keep `with_title("Harbor")`; set `with_title_background_color(None)` on Win11; after HWND creation apply `SetWindowThemeAttribute` with `WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON`; do not custom-draw or hit-test caption buttons.
- [ ] **Verification:** Manual smoke that title and icon are absent, system buttons work, and Alt-Tab/taskbar still identify Harbor.

## Approach

1. Keep the main window decorated and titled `Harbor` so the taskbar and Alt-Tab name stay intact; do not use `with_title("")`.
2. On the Win11 TransientWindow path from T0001, add `WindowAttributesExtWindows::with_title_background_color(None)` so DWM caption color is `DWMWA_COLOR_NONE`.
3. After the HWND exists, call `SetWindowThemeAttribute` (`WTA_NONCLIENT`) with `WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON` on the main window only.
4. Leave min/max/close to DWM; do not implement client-side decoration or custom caption painting.

## Blocked by

- T0001 — Caption chrome applies to the translucent Acrylic main window, not an opaque HWND.

## Blocks

- T0004 — Win10 accent and docs share `src/app.rs` window-creation code with this caption path.

## Acceptance

- [ ] Caption text is not drawn in the title strip.
- [ ] The window icon is not drawn in the title strip.
- [ ] Minimize, maximize, and close still work via system buttons.
- [ ] Taskbar and Alt-Tab still show Harbor.
- [ ] On Windows 11 22621+, the caption strip is Acrylic rather than an opaque accent bar.

## Out of Scope

- Custom-drawn caption buttons or CSD.
- Win10 accent-policy API (T0004).
- Changing confirmation-window decorations.
- Empty window title that would blank the taskbar name.
