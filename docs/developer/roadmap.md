# Roadmap

[Developer guide](README.md) · [Current progress](progress.md)

Harbor remains Windows-first: stabilize content, input and session lifecycle, then expand the workspace. The order below retains the previous roadmap's direction; it does not reopen completed implementations or approve additional scope. Detailed contracts remain in the [Grimoire specifications](../../.grimoire/spec/).

## Delivery order

| Stage | Goal | Current position |
| --- | --- | --- |
| M0 — Establish compatibility | Reproducible Windows application, code-page and extension-transport investigation | Still needed alongside correctness work; a probe or configured workflow is not broad compatibility. |
| M1 — Preserve content | Coherent Unicode, anchors and resize/reflow | Implementation is present, including the scoped no-wrap sequence-promotion repair. Integrated native/performance coverage remains. |
| M2 — Configure and discover | Reload, command palette and profiles | Reload, new-session launch settings and the existing-command palette are integrated. Native acceptance remains incomplete; profiles are not implemented. |
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
| N02 | Unicode text correctness | Combining/VS/ZWJ and the no-wrap two-cell promotion repair are implemented; integrated native coverage remains. |
| N03 | `chcp` / legacy Windows compatibility | [#154](https://github.com/wangjq4214/harbor/issues/154) is closed; [#171](https://github.com/wangjq4214/harbor/issues/171) still tracks overlapping code-page evidence and separate Kitty transport gates. Reconcile reusable evidence before repeating probes; closure alone is not a compatibility result. ConPTY remains UTF-8, not a GBK parser mode. |
| N04 | Configuration reload | Manual/watched reload and launch settings implemented; remaining integrated human/runtime acceptance. |
| N05 | Command palette | Existing visible commands integrated through the shared registry; [native acceptance and readability work remain](../../.grimoire/spec/0030-command-palette.md#delivery-and-outstanding-acceptance). #156 still defers profiles/panes/appearance scope. |
| N06 | Named session profiles | Not implemented; retain new-session-only launch policy. |
| N07 | Pane workspace | Not implemented; independent allocation, focus, routing, resize and close lifetimes required. |
| N08 | Scrollback search | Bounded current-session literal search delivered; retained acceptance/cost work and broader search scope remain separate. |
| N09 | Shell-integration workflows | OSC 7/8/133 metadata foundations exist; directory inheritance, command navigation and complete workflows remain. |
| N10 | Kitty keyboard | Not implemented; verify application-visible negotiation/input through ConPTY first. |
| N11 | Compatibility-driven extensions | Modern SGR, OSC palette, multi-encoding mouse and procedural boxes delivered as bounded slices, not full VT coverage. |
| N12 | Kitty graphics | Not implemented; transport feasibility, static-image subset, limits and lifecycle need separate delivery. |
| N13 | Theme and glass-style UI | Local primitives/search composition exist; unified product themes and chrome remain. |
| N14 | Liquid-glass refraction prototype | Bounded local edge-refraction primitive exists; measured product-level go/no-go and broader visual work remain. |
| N15 | History/performance budgets | Configurable dual history caps and 1024-byte text-unit protection implemented; closed by owner-approved waiver of remaining measurement/acceptance. [Spec 0031](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary) retains results/limits; panes remain separate under #158. |
| N16 | Diagnostics, acceptance and packaging | Logs/notices, MSI/ZIP and release workflows exist; integrated runtime, installer and published/downloaded-release closure remain. |
| N17 | Later workspace/platform work | Cross-window movement, restoration, persistent sessions and native Unix remain deferred. |

## Probe tracking boundary

Issue #154 is closed as completed, but its issue body/comments do not link a completed code-page result matrix. Issue #171 remains open and includes that matrix alongside separate keyboard and graphics transport investigations. Treat the code-page evidence as unverified here, not as either a demonstrated failure or a completed transport gate. Recover and check any earlier artifacts against the relevant Windows/ConPTY/application versions before rerunning only missing or inapplicable cases. Do not reopen #154 automatically or remove #171's acceptance criteria merely to align statuses.

Code-page findings cannot satisfy Kitty keyboard or APC graphics feasibility. Keep separate go/no-go/limited-support decisions for those paths; the existing capability-query probe is not evidence of Kitty negotiation or graphics delivery.

## Next work

1. Complete remaining integrated Windows correctness/lifecycle cases, including native acceptance of the sequence-promotion repair; do not reimplement delivered reflow, reload or search.
2. Complete comparable current-tree performance and targeted code-page/extension transport investigations. Record unavailable scenarios honestly; preserve payload confidentiality.
3. Continue discoverability and workspace work in dependency order: palette acceptance/profiles, independent panes, then richer shell workflows. Independent bounded compatibility or appearance fixes need not wait for every pane feature.

Keep one major implementation stream plus a short investigation/polish stream, rather than marking all future packages in progress. Advance release/platform claims only for the actual accepted scope. Test reports and run artifacts do not belong in this roadmap.
