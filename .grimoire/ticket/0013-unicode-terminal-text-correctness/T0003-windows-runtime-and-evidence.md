# Windows runtime and evidence for N02

**Ticket ID:** T0003
**Source:** [Spec 0015 R1–R4, Verification](../../spec/0015-unicode-terminal-text-correctness.md), [issue #153](https://github.com/wangjq4214/harbor/issues/153), [Validation](../../../docs/validation.md)
**Status:** Todo

## Goal

Provide reproducible, scope-specific evidence that combining and VS/ZWJ text behavior works in the integrated Windows terminal and clipboard path; update status/protocol documentation only for behavior actually implemented and observed.

## Affected surfaces

- **Windows integration:** ConPTY output in the running application, screen rendering/fallback, font/DPI changes, resize and clipboard selection/copy; named shell/application versions.
- **Evidence and claims:** `docs/verification/`, `docs/current-status.md`, `docs/protocol/checklist.md` and applicable N02 roadmap/issue references; quality-gate logs or linked retrievable CI artifacts.

## Approach

Exercise the final integrated revision of T0001 and T0002 with reproducible output fixtures, including separate PTY reads where observable, wide edges, overwrite/erase, fallback fonts, and repeated width changes. Record text/coordinate correctness separately from visual presentation and explicitly identify unsupported sequence rendering. Reuse N01 resize scenarios where helpful without equating N02 evidence to the outstanding N01 Windows/performance acceptance. Run applicable quality gates, capture exact commands and outcomes, then update only backed status/protocol claims; retain remaining scope and exclusions.

## Dependencies and coordination

- **Blocked by:** T0001 and T0002 for final acceptance against an integrated build; fixture preparation can happen earlier, but partial-build evidence cannot establish all #153 outcomes.
- **Blocks:** None.
- **Coordination risks:** Changes after capture invalidate some results; rerun affected scenarios and record the final revision/dirty-tree scope. N01 evidence has separate closure criteria.

## Acceptance

- [ ] Reproducible Windows shell and named application sessions verify combining, line-start cue, variation-selector and ZWJ examples on screen and via clipboard copy; include PTY fragmentation, right-edge placement, edits/erase, selection, repeated resize, font fallback and font/DPI changes as applicable to the delivered scope.
- [ ] Evidence distinguishes preserved source text and fixed cell width from successfully rendered whole-sequence presentation, and reports unsupported fonts or other known exclusions without blanket compatibility claims.
- [ ] Each scenario records revision, dirty-tree scope, OS/ConPTY/application versions, expected/observed result, artifacts, and honest PASS / FAIL / NOT RUN / BLOCKED status. Durable evidence is under `docs/verification/` or linked to retrievable CI artifacts without secrets or unrelated terminal content.
- [ ] Run and record results for `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`; record commands not run and why.
- [ ] Update status/protocol documentation only for implemented and evidenced N02 behavior, retaining deferred features and N01's separately outstanding runtime/performance acceptance. Close #153 only when all accepted scope is evidenced or explicit scope decisions link remaining/deferred work.

## Out of scope

This ticket does not implement missing Unicode behavior or independently close N01 acceptance. A passing parser/model suite alone is not Windows runtime evidence.
