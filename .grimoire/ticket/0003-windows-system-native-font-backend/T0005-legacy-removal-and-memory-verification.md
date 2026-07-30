# Legacy Removal and Memory Verification

**Ticket ID:** T0005
**Source:** [Spec: 0003-windows-system-native-font-backend](../../spec/0003-windows-system-native-font-backend.md)
**Status:** Todo

## Goal

The completed Windows font path contains no legacy loader or complete Rust-heap font-file copy and demonstrates a sub-40-MiB DHAT live-heap peak plus lower peak and steady private memory in the reference startup scenario.

## Layers

- [ ] **Font Sources:** Delete hard-coded platform candidates, direct filesystem font reads, the CJK probe/thread, and `fontdb` discovery from production font selection.
- [ ] **DirectWrite Backend:** Remove compatibility adapters, audit native resource release, and ensure every default, configured, and fallback path terminates in DirectWrite or an explicit error.
- [ ] **Text Core & CPU Atlas:** Remove `fontdue`-specific types/comments and temporary bridges, then audit caches to prove they retain native identities/bitmaps rather than complete font files.
- [ ] **Startup & Terminal Rendering:** Add low-volume measurement markers for font initialization, first presentation, first fallback, and steady-state dwell without changing the WGPU renderer.
- [ ] **Verification & Profiling:** Remove obsolete dependencies, update documentation, run quality gates, execute fixed DHAT/private-memory scenarios, and publish comparable before/after evidence.

## Approach

1. Delete the old `font.rs` candidate, `fs::read`, eager CJK thread/probe, `fontdb` database, and `fontdue` parsing branches after all native scenarios pass.
2. Remove `fontdb` and `fontdue` from `crates/harbor-text/Cargo.toml`, workspace dependencies when unused, and `Cargo.lock`; replace stale `fontdue` names in public types and comments.
3. Audit DirectWrite collection, face, fallback, and custom-file lifetimes so discovery-only objects and unused fallback faces are released promptly.
4. Add structured markers sufficient to separate primary selection, first present, first missing-glyph resolution, and the agreed steady-state dwell in profiling output.
5. Run the documented fixed scenarios: cold Latin launch, sustained Latin, first CJK, sustained CJK/symbol/emoji, and configured primary with CJK coverage.
6. Compare DHAT allocation/live-heap data and Windows peak/steady private memory against an identical pre-change executable scenario; record environment, font set, screen size, dwell, backend, and profiling mode.
7. Update README/roadmap statements that still describe hard-coded candidates, automatic CJK probe, `fontdb`, or `fontdue`; preserve the proposal document as historical rationale unless explicitly superseded.
8. Run formatting, clippy with warnings denied, focused tests, workspace tests, and source/dependency searches for forbidden production references.

## Blocked by

- T0001 — Provides the final contract that remains after compatibility removal.
- T0002 — Default system primary must be fully native.
- T0003 — Configured primary must be fully native.
- T0004 — Missing-glyph fallback must be fully native.

## Blocks

- (none)

## Acceptance

- [ ] Production `harbor-text` source contains no call path through `fs::read`, `fontdb`, `fontdue`, hard-coded platform font candidates, the CJK probe, or the eager CJK loader thread.
- [ ] `fontdb` and `fontdue` are absent from `harbor-text` and workspace dependency manifests when no other crate uses them; the lockfile reflects their removal.
- [ ] Non-Windows compilation fails with the explicit unsupported-platform diagnostic rather than compiling a portable legacy backend.
- [ ] Profiling finds no Rust allocation equal to a complete font file and no Harbor-owned complete font-file buffer retained at first present or steady state.
- [ ] The documented Latin DHAT run reports a global live-heap peak below 40 MiB on the reference machine.
- [ ] Peak and steady Windows private memory are both lower than the recorded pre-change run under the identical scenario.
- [ ] Default, configured, CJK/fallback, repeated-cache, invalid-config, redraw, resize, and DPI E2E scenarios pass.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, focused crate tests, and workspace tests pass on Windows.
- [ ] README and roadmap no longer describe the removed legacy font path as current behavior.

## Out of Scope

- Optimizing unrelated tracing, Winit, WGPU global caches, or PTY allocations found by the profile.
- Dynamic CPU/GPU atlas sizing, vertex scratch-buffer reuse, or a new eviction policy.
- Non-Windows font backends.
- Shaping, ligatures, bidi, grapheme layout, and color emoji.
- Introducing a timing gate for first presentation or first fallback; those values are recorded only.
