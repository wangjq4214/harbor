# Reorganize harbor-app Submodules and Flat Tests

**Ticket ID:** T0004
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Todo

## Goal

Clear all flat `*_tests.rs` files from `crates/harbor-app/src/` and group tightly coupled `workspace` and `tab_view` submodules while preserving all public `harbor_app` exports.

## Affected Surfaces

- `crates/harbor-app/src/lib.rs`
- Flat tests in `crates/harbor-app/src/`:
  - `palette_render_tests.rs` + `palette_render_tests/`
  - `palette_terminal_tests.rs`
  - `palette_tests.rs`
  - `pane_input_tests.rs`
  - `popup_tests.rs`
  - `profile_chooser_tests.rs`
  - `quick_save_tests.rs`
  - `search_tests.rs`
  - `workspace_tests.rs` + `workspace_tests/`
- Cohesive submodule grouping in `crates/harbor-app/src/`:
  - `palette.rs`, `quick_save.rs`, `quick_save_project.rs` -> under `workspace/`
  - `tab_view_ui.rs` -> under `tab_view/ui.rs` (or `tab_view/` submodule)

## Approach

- Group `palette.rs`, `quick_save.rs`, and `quick_save_project.rs` into `crates/harbor-app/src/workspace/` and `tab_view_ui.rs` into `crates/harbor-app/src/tab_view/`, preserving public re-exports in `crates/harbor-app/src/lib.rs` (such as `QuickSaveDraft`, `QuickSaveSubmission`, and any paths used by `harbor`, `harbor-app-ui`, or tests).
- Move each flat `*_tests.rs` file into its owning module's `tests.rs` / `tests/` (e.g., `search/tests.rs`, `popups/tests.rs`, `profiles/tests.rs`, `pane_layout/tests.rs`, `workspace/tests/`, `workspace/palette/tests.rs`, `workspace/quick_save/tests.rs`) or `crates/harbor-app/tests/` for purely black-box tests.
- Keep `gpu::` and any other CI test-filter module segments intact.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0006 (automated layout gate forbids `crates/harbor-app/src/*_tests.rs`).
- **Coordination risks:** Check `crates/harbor-app-ui/src/lib.rs`, root `src/`, and `docs/` for any internal or public path assumptions before moving `tab_view_ui` or `workspace` children.

## Acceptance

- [ ] Zero `*_tests.rs` files remain directly under `crates/harbor-app/src/`.
- [ ] `palette`, `quick_save`, and `quick_save_project` live under `crates/harbor-app/src/workspace/`, and `tab_view_ui` lives under `crates/harbor-app/src/tab_view/`.
- [ ] All public `harbor_app` API paths remain compatible for `harbor` and `harbor-app-ui`.
- [ ] `cargo test --locked -p harbor-app -p harbor-app-ui` and `cargo clippy --locked -p harbor-app -p harbor-app-ui --all-targets --all-features -- -D warnings` pass.

## Out of Scope

- Changing `harbor-app` UI behavior or breaking public `harbor_app` item paths.
