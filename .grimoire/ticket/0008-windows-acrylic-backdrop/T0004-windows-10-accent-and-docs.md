# Windows 10 accent and docs

**Ticket ID:** T0004
**Source:** [Spec: 0008-windows-acrylic-backdrop](../../spec/0008-windows-acrylic-backdrop.md)
**Status:** In Progress

## Goal

Windows builds below 22621, including Windows 10, show Acrylic in the main-window client through Default Background Cells, with opaque caption documented as accepted degradation, and P7/docs pointing at this behavior.

## Layers

- [ ] **Config:** None — tint already comes from `BACKGROUND` alpha 0.72.
- [ ] **Terminal:** None — compositing alpha and cell paint already land in T0001/T0003.
- [ ] **Winit Runtime Integration:** None — present/clear path is unchanged.
- [ ] **Runtime Host:** For build &lt; 22621, after HWND creation call `SetWindowCompositionAttribute` with `ACCENT_ENABLE_ACRYLICBLURBEHIND` and a gradient tint aligned to `BACKGROUND` at 0.72; keep system buttons; do not custom-draw caption.
- [ ] **Verification:** Document Windows 10 Caption Degradation and Acrylic as P7 product work; note confirmation stays opaque; Windows 10 smoke that client Acrylic does not crash.

## Approach

1. Reuse T0001's OS-build probe: 22621+ stays on TransientWindow; lower builds take the accent path instead of remaining a fully opaque HWND.
2. Resolve `SetWindowCompositionAttribute` from `user32.dll` via `GetProcAddress` (not a `windows` crate DWM feature). Apply `ACCENT_ENABLE_ACRYLICBLURBEHIND` with an ABGR tint matching `BACKGROUND` including alpha 0.72.
3. Keep `with_transparent(true)` on this path as needed for client alpha; skip opaque GDI first-paint the same as T0001.
4. Do not add CSD if the caption strip stays opaque; that is Windows 10 Caption Degradation.
5. Update `docs/roadmap.md` P7 product work and any validation/docs pointer so Acrylic and the Win10 caption limitation are recorded. State that paste confirmation is excluded.

## Blocked by

- T0001 — Accent path reuses translucency, compositing alpha, skipped GDI, and `BACKGROUND` tint.
- T0002 — Window-creation and caption-chrome edits in `src/app.rs` must be finished before adding the Win10 branch.
- T0003 — Closing smoke/docs assume Inverse Default Cells already paint correctly over Acrylic.

## Blocks

- None — final slice.

## Acceptance

- [ ] On Windows 10 (or Win11 &lt; 22621), Default Background Cells show Acrylic in the client area without crashing.
- [ ] An opaque caption strip, if present, is documented as accepted degradation; buttons remain system-drawn.
- [ ] `docs/roadmap.md` P7 (or linked validation docs) mentions Windows Acrylic behavior and the Win10 caption limitation.
- [ ] Paste confirmation remains an opaque separate window with unchanged alpha-mode selection.
- [ ] Quality gates in `docs/validation.md` stay green.

## Out of Scope

- Custom-drawn caption buttons or CSD to force Win10 caption Acrylic.
- Mica.
- Unix transparency.
- Confirmation-window Acrylic or alpha-mode change unless a black/opaque regression is proven.
- User TOML / runtime Acrylic toggle.
