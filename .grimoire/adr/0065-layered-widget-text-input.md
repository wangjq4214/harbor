# Layered Widget Text Input

**Status:** Testing
**Date:** 2026-10-16
**Sources:** The user's request for a reusable input component informed by Flutter, followed by approval of the proposed controller/editing-core/decorated-field split and complete single-line editing plus search migration. The selected endpoint is spec only, not implementation.
**Related:** [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0029](./0029-widget-store-state-action-boundary.md), [ADR 0031](./0031-widget-winit-adapter-owns-native-host-infrastructure.md), [ADR 0040](./0040-application-command-dispatch-and-keybinding-precedence.md).

## Context

At source inspection (`e3d8238a6c010b192944885a5e5d36913f79ca3c`), `SearchEntry` in `crates/harbor-app/src/workspace.rs` is an application-owned `PrimitiveLeaf`. It appends/removes text at the end and paints query/preedit text followed by a cursor-like character. It is not a reusable editor with movable caret and selection. The widget runtime has keyboard, focus, pointer capture, IME events/effects and clipboard writes; its native clipboard-read effect explicitly reports an unsupported result channel. Widget text rendering currently lays out individual characters, not an editing-position contract.

The approved direction is a reusable editor rather than relocation of the search-specific leaf. Flutter's `TextEditingController`, `EditableText` and `TextField` provide the reference separation of editing state, editing mechanism and decorated control. This is not a commitment to Flutter dependencies, Dart API compatibility or Material styling.

## Decision

1. **Separate editing state, editing mechanism and field presentation.** Use a controller for coherent text, caret/selection and IME composition state with programmatic read/write; a decoration-free editing core for input, focus, editing geometry and scrolling; and an ordinary field wrapper for placeholder, border, padding and disabled/read-only presentation. `TextEditingController`, `EditableText` and `TextField` are working names, not finalized Rust public signatures.
2. **Put reusable input in `harbor-widget`.** Its contracts must work without Harbor session, terminal or command types. This applies ADR 0064; search semantics, result navigation, query limits and product composition stay application-owned. The first application consumer will replace the current search-specific editing implementation rather than preserve a second editor beside it.
3. **Keep local editing state distinct from application business state.** The controller represents editing state, not an application reducer or a second search engine. The application observes committed text and translates generic edit/submit intent through its established command boundary. IME composition is transient editing state, not a committed search query.
4. **Keep platform operations at the existing host boundary.** The editing core uses generic runtime/platform contracts for clipboard and IME. Completing clipboard readback and editing-position support is part of delivering a usable input; merely rendering a field or emitting the current unsupported read effect is insufficient. No terminal-only adapter or new global event bus is selected.

## Consequences

- A plain editor and a decorated field share one editing mechanism, while consumers retain control of composition and appearance.
- Search can gain caret, selection, clipboard, undo/redo and Unicode-safe editing without making search commands or the 128-byte query policy widget concerns.
- Runtime focus/routing, text geometry and host clipboard/IME integration must support an actual editor. Existing terminal input, clipboard authorization, application command and HMR ownership contracts remain applicable.
- Exact public API spelling, storage/index representation, history grouping and internal module layout remain implementation details, not decisions in this record. The required behavior and first-delivery scope are in [spec 0028](../spec/0028-reusable-single-line-text-input.md).
- This records the approved direction only. No implementation, test pass or native-runtime acceptance is established by the ADR.
