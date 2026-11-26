# Organize Documentation and Scripts Layout

**Ticket ID:** T0001
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Todo

## Goal

Ensure `docs/` and `scripts/` contain only `README.md` plus domain/purpose subdirectories at their roots, with every internal Markdown link updated and the fixed repository layout documented in `docs/developer/architecture.md`.

## Affected Surfaces

- `docs/flex-layout.md` -> `docs/widget/flex-layout.md`
- `docs/widget-input.md` -> `docs/widget/widget-input.md`
- `docs/README.md`, `docs/widget/README.md`, `docs/developer/architecture.md`, and any `docs/` or `.grimoire/` files linking to `flex-layout.md` or `widget-input.md`
- `scripts/README.md` (index `scripts/shell/`)

## Approach

- Move `docs/flex-layout.md` and `docs/widget-input.md` into `docs/widget/`.
- Search all tracked Markdown files (`.grimoire/` and `docs/`) for references to `flex-layout.md` and `widget-input.md` and update relative paths and anchors.
- Add the `shell/` directory entry to the table in `scripts/README.md` and add a concise repository layout section to `docs/developer/architecture.md` summarizing top-level ownership, module style, and test placement rules.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0006 (automated layout gate requires `docs/` and `scripts/` compliance).
- **Coordination risks:** Update `.grimoire/` references (such as `CONTEXT.md` or specs/ADRs if any cite `docs/flex-layout.md` or `docs/widget-input.md`) carefully so `scripts/docs/check_docs.py` passes without introducing non-English text.

## Acceptance

- [ ] `docs/` contains only `README.md` and `app/`, `developer/`, `terminal/`, `widget/` at its root.
- [ ] `scripts/README.md` documents `scripts/shell/`.
- [ ] `docs/developer/architecture.md` records the standardized repository layout conventions.
- [ ] `python scripts/docs/check_docs.py` exits `0` with zero broken links or language warnings.

## Out of Scope

- Reorganizing Rust source files or adding `scripts/ci/check_layout.py`.
