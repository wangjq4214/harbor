# Windows Integrated Acceptance

**Ticket ID:** T0003
**Source:** [Spec 0022 R5 and integrated Definition of Done](../../spec/0022-box-drawing-and-decoration-lifecycle.md), [Validation Policy](../../../docs/validation.md), [historical modern SGR acceptance](../../../docs/modern-sgr-acceptance.md), [Issue #193](https://github.com/wangjq4214/harbor/issues/193).
**Status:** Todo

## Goal

Establish truthful integrated Windows evidence for box drawing, native changed-metrics font-resource replacement, and actual cross-monitor DPI behavior, with fresh quality gates and accurate documented scope.

## Affected Surfaces

- Windows application workload, native DirectWrite/wgpu resource-smoke fixture/example or test using `TerminalRenderPipeline::replace_fonts`, and actual Harbor desktop DPI transitions.
- A self-contained acceptance record in `docs/` and affected current-status/protocol/contract links where needed.
- Existing T0001/T0002 fixtures and tests as execution inputs; this ticket is not a separate production feature or global refactor.

## Approach

Validate the combined implementation against Spec 0022 and run the required quality commands. Name the actual application, environment, font configuration, workload and procedures. Keep application visuals, native font-resource replacement, actual cross-monitor observations and automated geometry/GPU evidence separate. Historical NOT RUN cases remain historical; new evidence identifies its own revision and scope.

Native font smoke uses Windows DirectWrite and actual GPU rendering with a configuration that demonstrably changes metrics, observing ordinary glyphs, box drawing and all three decoration kinds. It does not require or claim user-settings hot reload. Cross-monitor smoke requires a real move between differing display DPI/scale settings, not only offscreen scale mutation.

## Dependencies and Coordination

- **Blocked by:** T0001 for final box-drawing workload acceptance and T0002 for final metric/decoration lifecycle acceptance. Both are required before final combined-tree claims.
- **Blocks:** None.
- **Coordination:** Procedure/environment preparation may happen earlier. If native conditions are unavailable, preserve NOT RUN/BLOCKED and seek an explicit scope decision before completion by deferral. New semantic/architecture decisions or demonstrated failures return to refinement/the responsible implementation work rather than being hidden in the evidence.

## Acceptance

- [ ] A named Windows application workload with actual box-drawing borders is exercised for appearance/redraw/resize. Record the application's version and transport; synthetic fixtures are labeled separately and do not substitute for this workload.
- [ ] Windows native font-resource replacement with genuinely changed metrics is observed through DirectWrite and GPU rendering; before/after glyphs, box joins, underline styles, strikethrough and overline remain aligned. State the procedure's resource-level scope and absence of user-config hot reload.
- [ ] Actual Harbor window moves between monitors with differing DPI/scale are observed and recorded; no double scaling, stale allocation/origin geometry or decoration misalignment is present.
- [ ] Each unavailable required native/DPI case has a concrete NOT RUN/BLOCKED reason. Completion by deferral additionally includes an explicit authorized scope decision. FAIL remains a defect/blocker, not a pass or an unapproved deferral.
- [ ] T0001/T0002 focused CPU/GPU results and affected source/copy/width, ordinary/emoji/combining/preedit, SGR/OSC 8/conceal, dirty/resize and retained invalidation regressions are included in the integrated evidence at their actual tested scope.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py` are freshly executed successfully on the combined tree; record revision/dirty scope and actual results, not inherited historical passes.
- [ ] Evidence follows `docs/validation.md`: environment, Windows/application versions, font/viewport/scale, reproducible commands/steps, expected/observed results and separate PASS/FAIL/NOT RUN/BLOCKED outcomes and limitations.
- [ ] Committed summaries are self-contained and do not link local-only raw artifacts. Historical acceptance is not overwritten; any affected current support claims and local links accurately distinguish implemented behavior, executed tests and native acceptance.
- [ ] Spec 0022's complete outcome/constraint coverage is checked before claiming #193 complete; no broader protocol/platform or settings-reload support is inferred.

## Out of Scope

New production features, settings hot reload, universal font/platform/transport certification, unrelated #162/N11 acceptance, silently deferring required evidence, or treating configured CI/compilation/simulated DPI as native runtime PASS.
