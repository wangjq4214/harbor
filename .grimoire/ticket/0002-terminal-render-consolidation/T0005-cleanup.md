# Cleanup

**Ticket ID:** T0005
**Source:** [Spec: 0002-terminal-render-consolidation](../../spec/0002-terminal-render-consolidation.md)
**Status:** Done

## Goal

Delete `harbor-render` and `harbor-ui` crates. Remove residual old-architecture code: `Component` trait, `UiRoot`, `EventResult` (if unused), `ModalContent`, old `terminal_worker` communication path. App layer is clean — depends only on `harbor-widget` + `harbor-terminal` + `harbor-parser`.

## Layers

- [ ] **Parser:** None — complete
- [ ] **Terminal:**
  - Remove any residual references to `harbor-render`
  - `EventResult` and `Component` trait: if moved into terminal, ensure they are not publicly exported (keep private)
- [ ] **Widget:**
  - Remove any references to `harbor-render` (comments, docs, indirect deps)
  - Remove any references to `harbor-ui`
- [ ] **App:**
  - `src/app.rs` — delete `use harbor_render::*`, `use harbor_ui::*`
  - `src/app/ui.rs` — **delete file** (UiRoot no longer used)
  - `src/app/confirmation.rs` — remove `harbor_render::GpuContext` reference (import from `harbor-terminal` if still needed)
  - `src/app/input.rs` — remove harbor_render references (`render_csi_key`, etc.)
  - `Cargo.toml` (workspace root) — remove `harbor-render`, `harbor-ui` from members
  - If `terminal_worker` is no longer used, remove the module

## Approach

1. **Verify consumers**: `cargo tree -p harbor-render` and `cargo tree -p harbor-ui` — confirm no consumers remain beyond app.
2. **Delete crates**:
   - `rm -rf crates/harbor-render/`
   - `rm -rf crates/harbor-ui/`
   - Remove both from workspace `Cargo.toml` `[workspace.members]`
3. **App code cleanup**:
   - `src/app/ui.rs` — delete file
   - `src/app.rs` — remove `mod ui;`, remove `UiRoot`-related fields and methods, remove `harbor_render::*` imports
   - If `confirmation.rs` depends on `harbor_render::GpuContext`: re-import from `harbor-terminal::gpu::GpuContext` (if migrated) or from `harbor-types`
   - `src/app/input.rs` — remove `harbor_render::*` imports; if `render_csi_key` still needed, import param types from terminal
4. **Residual cleanup**:
   - `Component` trait — if now internal to terminal: make `pub(crate)` or private
   - `EventResult` — if only used inside terminal: privatize. If app still needs it (e.g. confirmation dialog return type), keep in terminal's public exports
   - `UiRequest`, `InteractionResult`, `WaitResult` — confirm which are still used; migrate remaining ones to `harbor-terminal` or `harbor-types`
5. **Build verification**: `cargo build --workspace` succeeds, `cargo test --workspace` passes.
6. **Lint check**: `cargo clippy --workspace` no new warnings (expect `dead_code` warnings after crate deletion).

## Blocked by

- T0004 — PTY I/O and input must fully work before dead code can be identified

## Blocks

(None — final ticket)

## Acceptance

- [ ] `crates/harbor-render/` directory does not exist
- [ ] `crates/harbor-ui/` directory does not exist
- [ ] Workspace `Cargo.toml` does not reference harbor-render or harbor-ui
- [ ] `cargo build --workspace` succeeds
- [ ] `cargo test --workspace` all pass
- [ ] `src/app/ui.rs` deleted
- [ ] `src/app.rs` has no `use harbor_render::*` or `use harbor_ui::*`
- [ ] No broken imports (IDE/compiler reports no errors)
- [ ] Confirmation dialog functionality not regressed (temporary degradation is acceptable; note TODO if present)

## Out of Scope

- Paste confirmation dialog widget reimplementation (separate future spec/ticket)
- `harbor-pty` crate refactor (may be kept; only the communication path changed from worker channels to Terminal directly holding PTY handles)
- Any new features
