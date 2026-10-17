# Roadmap

[Developer guide](README.md) · [Current progress](progress.md)

Harbor remains Windows-first: stabilize content, input and session lifecycle, then expand the workspace. The order below retains the previous roadmap's direction; it does not reopen completed implementations or approve additional scope. Detailed contracts remain in the [Grimoire specifications](../../.grimoire/spec/).

## Delivery order

| Stage | Goal | Current position |
| --- | --- | --- |
| M0 — Establish compatibility | Reproducible Windows application, code-page and extension-transport investigation | Still needed alongside correctness work; a probe or configured workflow is not broad compatibility. |
| M1 — Preserve content | Coherent Unicode, anchors and resize/reflow | Implementation is present. Resolve the known selector-width discrepancy and remaining native/performance coverage. |
| M2 — Configure and discover | Reload, command palette and profiles | Manual/watched reload and new-session launch settings are implemented. Integrated acceptance, palette and profiles remain. |
| M3 — Work in panes | Independent panes, search and shell workflows | Current-session literal search is implemented. Panes and broader shell workflows are not. |
| M4 — Extend compatibility | Selected VT gaps, Kitty keyboard and bounded graphics | Modern SGR, palette and mouse slices exist; Kitty negotiation/transport and graphics remain investigation-gated. |
| M5 — Polish | Unified product themes/glass and release hardening | Acrylic and search-panel blur/gradient/refraction primitives exist. Whole-product composition and measured acceptance remain. |
| M6 — Expand workspace/platform | Cross-window movement, restoration/persistence and native Unix | Deferred behind relevant Windows stability, lifecycle and resource-ownership work. |

Performance, diagnostics, parser safety and representative Windows checks apply throughout, not only at the end.

## Retained work-package map

These identifiers preserve references used by existing issues/specifications without restoring the old lengthy product-plan document.

| ID | Work package | Current state / next boundary |
| --- | --- | --- |
| N01 | Resize reflow | Implemented; integrated Windows and large-history cost closure remain. |
| N02 | Unicode text correctness | Combining/VS/ZWJ implemented; no-wrap selector-width discrepancy and native coverage remain. |
| N03 | `chcp` / legacy Windows compatibility | Investigate actual console API and byte paths. ConPTY remains UTF-8; do not switch the VT parser to GBK based on `chcp`. |
| N04 | Configuration reload | Manual/watched reload and launch settings implemented; remaining integrated human/runtime acceptance. |
| N05 | Command palette | Not implemented; build on the existing typed command registry, not a second command bus. |
| N06 | Named session profiles | Not implemented; retain new-session-only launch policy. |
| N07 | Pane workspace | Not implemented; independent allocation, focus, routing, resize and close lifetimes required. |
| N08 | Scrollback search | Bounded current-session literal search delivered; retained acceptance/cost work and broader search scope remain separate. |
| N09 | Shell-integration workflows | OSC 7/8/133 metadata foundations exist; directory inheritance, command navigation and complete workflows remain. |
| N10 | Kitty keyboard | Not implemented; verify application-visible negotiation/input through ConPTY first. |
| N11 | Compatibility-driven extensions | Modern SGR, OSC palette, multi-encoding mouse and procedural boxes delivered as bounded slices, not full VT coverage. |
| N12 | Kitty graphics | Not implemented; transport feasibility, static-image subset, limits and lifecycle need separate delivery. |
| N13 | Theme and glass-style UI | Local primitives/search composition exist; unified product themes and chrome remain. |
| N14 | Liquid-glass refraction prototype | Bounded local edge-refraction primitive exists; measured product-level go/no-go and broader visual work remain. |
| N15 | History/performance budgets | Bounded content/search APIs exist; current measured budgets, multi-session residency and comparable captures remain. |
| N16 | Diagnostics, acceptance and packaging | Logs/notices, MSI/ZIP and release workflows exist; integrated runtime, installer and published/downloaded-release closure remain. |
| N17 | Later workspace/platform work | Cross-window movement, restoration, persistent sessions and native Unix remain deferred. |

## Next work

1. Close the known content discrepancy and remaining integrated Windows correctness/lifecycle cases; do not reimplement delivered reflow, reload or search.
2. Complete comparable current-tree performance and targeted code-page/extension transport investigations. Record unavailable scenarios honestly; preserve payload confidentiality.
3. Continue discoverability and workspace work in dependency order: command palette/profiles, independent panes, then richer shell workflows. Independent bounded compatibility or appearance fixes need not wait for every pane feature.

Keep one major implementation stream plus a short investigation/polish stream, rather than marking all future packages in progress. Advance release/platform claims only for the actual accepted scope. Test reports and run artifacts do not belong in this roadmap.
