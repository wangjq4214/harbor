# Current-Session Query, Highlight and Navigation Integration

**Ticket ID:** T0003
**Source:** [Spec 0027](../../spec/0027-revisioned-terminal-read-model-and-literal-search.md), R2–R9; Result acceptance and activation / Decoration projection / Shell compatibility seams and approved minimal current-session scope.
**Status:** Todo

## Goal

A user can enter a literal query in the current session, see valid highlights and navigate previous/next matches in retained content without corrupting output. The real application rejects stale/wrong-session completions before publication and revalidates stored targets at activation.

## Affected Surfaces

- Application-owned composition, query state and command policy: `crates/harbor-app/src/{command,workspace,tab_manager}.rs` and relevant application UI; `TabId`/`TabManager`.
- Host routing/side-effect boundary: root `src/{event,shell,tab_coordinator}.rs`.
- `TerminalWidgetBridge` in `crates/harbor-app/src/terminal_view.rs` and concrete `crates/harbor-terminal/src/render/` projection, including `pipeline.rs`; generic widget mechanisms only where needed.
- Focused application/bridge/render/routing tests, including existing `crates/harbor-app/src/workspace_tests.rs` as applicable.

Alignment: [ADR 0040](../../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [0041](../../adr/0041-structured-keybinding-configuration.md), [0064](../../adr/0064-widget-and-application-component-ownership.md), [0045](../../adr/0045-gpu-independent-terminal-core-boundary.md), [0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), and [0037](../../adr/0037-osc133-shell-integration-metadata.md).

## Approach

Compose the minimal query entry and previous/next actions in the application over T0002, keeping source reads/matching outside paint/hover. Bind results to actual stable session and buffer/content/query provenance. Use the established command/routing boundary for user-invokable commands and retain consumed versus pass-through semantics.

Publish terminal-domain decoration data only after validation; project currently valid ranges with current renderer geometry. Navigation uses the same provenance and validates again immediately before acting. Reading/matching never executes a target. Concrete delivered interaction details must be documented with tests; placement/default chords and incidental module design are not decided here.

## Dependencies and Coordination

- **Blocked by:** [T0002](./T0002-bounded-literal-search-consumer.md): real UI/result gates need the bounded consumer and query/provenance/status contract (and transitively T0001).
- **Blocks:** [T0004](./T0004-integrated-acceptance-and-evidence.md): application/runtime evidence requires this actual end-to-end path.
- **Coordination:** Bridge/render edits must preserve `read_update`/`acknowledge_update` semantics and prepared-result rendering. Application policy stays application-owned; do not use shared files to justify new blockers or generic business controllers.

## Acceptance

- [ ] **A1 — Usable current-session slice (R6/R7):** Document and test actual query entry, query change/cancellation, visible matching highlights and previous/next navigation into retained content. Use shared bounded search, expose incomplete/limited coverage honestly and preserve live output. No independent full-history search path or global cross-session search is introduced.
- [ ] **A2 — Actual acceptance and activation gates (R2/R4/R5/R7/R9):** Through the real application path, validate current query, live session identity, buffer/content generation/revision and retained text/range before accepting asynchronous results for decoration. Validate again before activating a stored target. Delay a completion and mutate text at a still-resolvable anchor; then mutate after presentation but before navigation. Neither stale highlight publication nor stale activation is allowed.
- [ ] **A3 — Session/query lifecycle regressions (R2/R4/R6/R7/R9):** Test session switch, close/replacement, primary/alternate transitions with saved-primary resize, hard reset/buffer replacement and older-query completion after replacement/cancellation through real host routing. Wrong-session/buffer events cannot decorate or navigate unrelated active content; closed/replaced targets cannot act. Current tab/ring slot is not sufficient identity. Do not add EOF-only invalidation as a requirement.
- [ ] **A4 — Neutral current-geometry rendering (R3/R5/R8/R9):** Render prepared valid terminal-domain ranges with current geometry without rescanning/copying full history on frames or hover. Test soft-wrap/hard-break projection, multi-batch logical lines, CJK/combining/emoji complete units and generated padding/cue exclusion. Reflow never reuses old physical coordinates. Highlighting changes no source text/cells, parser state or SGR; selection, cursor and preedit remain compatible; read/paint does not consume unrelated pending damage/events.
- [ ] **A5 — Command and ownership regressions (R5/R7/R9):** User-invokable search commands use existing registry/typed dispatch and scoped routing. Tests prove consumed search input emits no PTY bytes and pass-through behavior remains intact, including existing context-sensitive terminal commands. Document delivered invocation/binding semantics without assuming default chords here. Product composition/activation stays application-owned; reusable widget APIs remain generic. Existing OSC 8 activation policy and OSC 7/133 parsing remain unchanged.
- [ ] **A6 — Progress and invalidation in integration (R4/R6/R9):** Tests exercise output append/overwrite/erase/structural changes while searching, width/height changes/reflow, whole/partial eviction and ring reuse. Lost/changed text cannot remain actionable. Deliver documented remapping or deterministic invalidation with fresh bounded work; demonstrate useful current-query results while output continues. Preserve buffer-specific resizing and ConPTY live-primary/history retention exceptions, not lossless restoration of clipped tails.

## Out of Scope

Full #159 result-preservation scope, full #160 command/directory workflows, URL/file detection/opening, new OSC parsing, regex/advanced case options, global search, panes, a global event bus, new crate split, universal renderer hosting or mandatory reflow remapping. T0004 supplies runtime/evidence closure, not this ticket's first regression tests.
