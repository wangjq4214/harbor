# Verify and Reconcile the Complete Configuration Reload Package

**Ticket ID:** T0004
**Source:** [Spec 0024 R1–R6 and E1–E12](../../spec/0024-watched-reload-and-launch-settings.md), [spec 0023](../../spec/0023-atomic-manual-configuration-reload.md), [#155](https://github.com/wangjq4214/harbor/issues/155), [validation policy](../../../docs/validation.md).
**Status:** Todo

## Goal

Demonstrate the integrated package: manual/default-on watched reload, fonts/colors/bindings, new-session shell/environment/directory and explicit restart-required settings, with honest failure and retrievable Windows evidence.

## Affected Surfaces

Combined application/native tests, controlled failure fixtures, Windows runtime workloads, documentation/status/roadmap/config examples and issue-delivery evidence. Repair confirmed integration defects within this contract, not unrelated product scope.

## Dependencies and Coordination

- **Blocked by:** T0001 supplies accepted manual/font baseline; T0002 supplies native environment/directory behavior; T0003 supplies watcher behavior.
- **Blocks:** No local ticket; its accepted result supports #155 closure.
- Keep per-slice evidence with its original revision; establish fresh results for the integrated revision rather than assuming independent passes compose.

## Acceptance

- [ ] All spec 0024 requirements and E1–E12 are mapped to exact evidence; spec 0023's existing outcomes remain covered.
- [ ] E10 interleaves manual requests, ordinary/atomic/burst saves and all live/new-session settings across active/inactive/alternate sessions and newly created sessions.
- [ ] Invalid launch fields, invalid TOML/bindings, font/resource failures and multi-session resize/compensation failures do not publish partial accepted settings or terminate existing PTYs; diagnostics identify actual recovery limitations.
- [ ] Actual Windows shell child probes verify env/cwd, real editors or reproducible equivalent replacement sequences verify watching, and the required cmd/PowerShell/nvim/input/IME/font scenarios have explicit results.
- [ ] Existing clipboard restart policy, protected TERM, literal/empty/removal semantics, relative-directory anchoring, lost-directory spawn failure and environment-value confidentiality survive integrated operation.
- [ ] Execute `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`; retain command results and limitations.
- [ ] Evidence contains revision/dirty scope/OS/ConPTY/app versions, reproducible steps, expected/observed outcomes, artifact links and PASS/FAIL/NOT RUN/BLOCKED labels; local-only ignored paths are not presented as retrievable evidence.
- [ ] README, config example, current status and roadmap distinguish implemented/evidenced/deferred behavior and contain working evidence references.
- [ ] With separate remote-action authorization, reconcile #172's font scope and #155's delivery links/sub-issue status. Otherwise report the precise pending remote actions; do not claim the remote tracker is complete.
- [ ] Close #155 only with accepted evidence for the whole approved scope, or a new explicit user scope decision linking all remaining work. Environment/directory cannot be silently deferred.

## Out of Scope

New feature requirements, blanket interoperability claims, automatic approval of failed/skipped gates, unapproved Git ignore changes, or closing issues without execution authorization.
