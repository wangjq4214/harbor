# Integrated Windows Evidence and Truthful Profile Documentation

**Ticket ID:** T0004
**Source:** [Spec 0052 R8 and E1–E14](../../spec/0052-named-session-profiles-and-quick-save.md); [#157](https://github.com/wangjq4214/harbor/issues/157).
**Status:** Todo

## Goal

Establish retrievable evidence for the delivered complete profile/quick-save scope, document actual user behavior and exclusions, and hand back an honest package disposition. This is not permission to mark unrun cases passing or to close #157 without its evidence/remaining-scope requirements.

## Affected Surfaces

Integrated Windows/ConPTY and UI regression execution, quality-gate records, `docs/verification/` or retrievable CI artifacts, and applicable user configuration/getting-started/troubleshooting/developer status documents.

## Dependencies and Coordination

- **Blocked by:** T0002's actual chooser/entry paths and T0003's actual save/confirmation/persistence flow; both are needed for combined launch → capture → save → reopen behavior.
- **Coordination:** Use the exact integrated revision/dirty scope, including T0001 launch policy. Reuse valid focused evidence without treating historical unrelated acceptance as this feature's runtime result.
- **Remote tracking:** Obtain explicit authorization before creating/updating implementation sub-issues, issue links or closure. Local ticket completion is not an implicit remote mutation request.

## Acceptance

- [ ] Exercise E1–E14 with synthetic profiles, including both selection entry paths and save button/Enter, exact actual-child argv/env/cwd, local/remote OSC 7 safety, legacy launch compatibility and no existing-child mutation.
- [ ] Exercise Chinese IME, held/repeated Enter, popup replacement/stale events, resulting focus and consumed-input isolation; observe that save confirmation creates no new native window.
- [ ] Verify default/opt-in environment policy and stable save provenance across tab/directory/profile changes, without logging/disclosing values or unrelated terminal content.
- [ ] Demonstrate successful save → chooser selection → child launch and restart reload with unrelated settings/comments preserved, installed/portable isolation and no duplicate/stale watcher publication.
- [ ] Record conflict/invalid-file/name-race/permission/replacement-fault outcomes and input retention; any unexercised failure case is explicitly NOT RUN/BLOCKED, not inferred from happy-path success.
- [ ] Record revision, dirty-tree scope, OS/ConPTY/application versions, reproducible steps, expected/observed outcomes, artifact locations and known exclusions using PASS / FAIL / NOT RUN / BLOCKED.
- [ ] Record `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, `python scripts/checklist_summary.py`, including not-run commands and reasons.
- [ ] Update user docs with the implemented configuration spelling/defaults, command names, directory precedence/locality, ordinary-vs-profile behavior, save capture/env/confirmation/write rules, portable targeting and actual recovery limitations.
- [ ] Status/protocol claims match evidence; retain all deferred/missing scope explicitly. #157 remains a work-package tracker until accepted scope is evidenced or an explicit linked scope disposition covers the rest.

## Out of Scope

Inventing new requirements during validation, broad Windows/application compatibility claims, silently fixing unrelated packages, forced issue closure, credentials/real user content in evidence, or marking document creation as runtime validation.
