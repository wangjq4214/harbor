# Parser API Shrink

**Ticket ID:** T0001
**Source:** [Spec: 0002-terminal-render-consolidation](../../spec/0002-terminal-render-consolidation.md)
**Status:** Todo

## Goal

Shrink `harbor-parser`'s public API to exactly three items: `VtHandler` trait (renamed from `Perform`), `Params` type, and `Parser` struct. All internal state (`CsiAccumulator`, `Utf8State`, `State` enum) becomes fully private. `harbor-terminal`'s parser adapter layer adapts to the new signatures.

## Layers

- [ ] **Parser:** Rename `Perform` → `VtHandler`, simplify method signatures; keep `Params` public but hide fields; `Parser` struct unchanged; make `CsiAccumulator` and `Utf8State` private
- [ ] **Terminal:** `crates/harbor-terminal/src/parser/handlers.rs` — change `ScreenHandler` from `impl Perform` to `impl VtHandler`; adapt method signatures
- [ ] **Widget:** None — does not touch harbor-widget
- [ ] **App:** None — does not touch src/

## Approach

1. `harbor-parser/src/perform.rs` — rename to `VtHandler`, reorganize methods:
   - Keep `print(char)`, `execute(u8)`, `osc_dispatch(&[&[u8]], bool)`
   - `csi_dispatch` keeps CSI intermediates and replaces marker identity with a DEC-private flag: `csi_dispatch(&mut self, params: &Params, intermediates: &[u8], action: u8, private: bool)`. `private` is true only for the DEC `?` marker; non-DEC private markers are consumed without being reinterpreted as standard CSI.
   - `esc_dispatch` drops only `ignore` and keeps intermediates: `esc_dispatch(&mut self, intermediates: &[u8], byte: u8)`. Parser suppresses malformed ESC sequences before dispatch.
   - `hook` → `dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8)`. Parser suppresses malformed DCS introducers before dispatch.
   - `put` → `dcs_put(&mut self, byte: u8)`
   - `unhook` → `dcs_unhook(&mut self)`
   - Keep `start_string(&mut self, kind: u8)`
2. `harbor-parser/src/lib.rs` — public exports: `pub use core::Parser; pub use params::Params; pub use perform::VtHandler;`. Remove all other `pub` exports.
3. `harbor-parser/src/core.rs` — `Parser` and `State`: keep `State` enum private. Change `advance` generic bound from `P: Perform` to `H: VtHandler`.
4. `harbor-parser/src/params.rs` — `Params` stays public with private representation and public opaque accessors for parameter count and sub-parameters; `CsiAccumulator` and `Utf8State` become `pub(crate)`.
5. `harbor-terminal/src/parser/handlers.rs` — `impl Perform for ScreenHandler` → `impl VtHandler for ScreenHandler`. Rename methods, adapt signatures.
6. `harbor-terminal/src/parser.rs` — `use harbor_parser::Perform` → `use harbor_parser::VtHandler`.
7. Tests: adapt within `crates/harbor-parser/` for new trait name; adapt `crates/harbor-terminal/` parser incremental tests.

## Blocked by

(None — pre-refactoring ticket)

## Blocks

- T0003 — Terminal absorb rendering depends on new VtHandler trait signatures

## Acceptance

- [ ] `harbor-parser` public API is exactly three items: `VtHandler`, `Params`, `Parser`
- [ ] `CsiAccumulator` and `Utf8State` are not visible outside the crate
- [ ] `harbor-terminal` compiles and all parser tests pass with pre-refactor VT behavior unchanged, including CSI/ESC/DCS sequences that use intermediates
- [ ] `cargo build -p harbor-parser` succeeds with no public type leak warnings
- [ ] Existing terminal tests (`terminal_tests.rs`, `parser/tests.rs`, `parser/incremental_tests.rs`) all pass

## Out of Scope

- Adding or removing `VtHandler` methods (rename and re-sign only, no net change in count)
- Redesigning the opaque `Params` representation beyond the accessors required to replace its hidden fields
- Parser state-machine, UTF-8 decoding, and OSC/DCS buffering behavior changes; dispatch filtering only replaces the removed `ignore` callback argument
