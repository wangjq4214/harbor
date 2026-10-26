# Preserve Click Naming and Standardize Disabled Builders

**Status:** Proposed
**Date:** 2026-10-23

## Context

The historical UI/API review F3 recommends both UI-thread callback consistency and public API renaming: activation callbacks to `on_activate` and availability setters to `enabled`. These are separable choices. Button and IconButton already expose `on_click` and `disabled`; TextField and its EditableText core expose `enabled`.

During F3 refinement, the user rejected renaming the button APIs and approved aligning TextField and EditableText with the existing negative availability setter instead. This records the agreed public API direction, not implementation or test acceptance. [ADR 0065](./0065-layered-widget-text-input.md)'s controller/core/field boundaries remain unchanged; that record leaves exact public API spelling open.

## Decision

1. **Retain `on_click` on Button and IconButton.** The name denotes logical button activation, including the existing mouse and Enter/Space paths; renaming is not necessary to describe keyboard support. Preserve the existing interaction behavior rather than redesign activation timing as part of this naming change.
2. **Align ordinary control availability builders on `disabled(bool)`.** Controls remain usable by default (`disabled == false`); `disabled(true)` disables them. TextField and EditableText adopt this direction rather than requiring Button, IconButton and InteractiveRegion to switch to `enabled`.
3. **Preserve behavior when migrating polarity.** A call to `enabled(value)` becomes `disabled(!value)`. Read-only remains distinct from disabled; the spelling change must not change editing, selection, clipboard or focus behavior.

## Consequences

- The naming clauses of the F3 review recommendation are deliberately revised: consistency follows the existing button API, not the proposed positive setter and activation name.
- TextField, EditableText, their consumers and relevant documentation/tests need migration. No implementation is established by this record.
- **Refinement update (2026-10-23):** Callback ownership was left separate at the naming decision and has since been agreed in [ADR 0082](./0082-ui-thread-local-widget-callbacks.md). This record does not prescribe focus-handle mechanics or compatibility aliases.
- No unrelated `enabled` API is automatically included merely because it uses the same word. Existing InteractiveRegion `on_activate` is not renamed by this decision.
