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

## Execution Update — 2026-10-15

Automated integration and bounded Windows window checks are complete on `0a4057fd1c61f8d26f0001c0a3ef37656e9131fa` plus the T0004 test/documentation diff; full interactive acceptance is still pending. No production behavior or dependency changed. The new [combined E10 test](../../../src/session_launch_tests.rs) exercises real file watching, manual/save ordering, active/parked alternate buffers, GPU/font/binding/factory coherence, actual new-child environment/directory, prior-child continuity, strict rejection and resource-failure retry. Existing native compensation and launch-policy regression tests were refreshed on the integrated tree.

All five required gates pass: fmt, clippy with warnings denied, workspace tests (2465 passed / 0 failed / 10 ignored), documentation checks and checklist inventory. Windows 26300.9457 / ConPTY 1.24.260710001 / Harbor 0.1.0 default debug checks observed ordinary/replacement/burst saves, font/color transitions, new cmd/PowerShell launch probes, and exactly `7871` (x/q) through a raw ConPTY input probe across successful and rejected manual reloads. Native nvim content rendered across active/inactive reloads, but automated editing/exit sequences were unreliable; this is not full nvim/input acceptance. See the existing [test sources](../../../src/configuration_reload_tests.rs) and [validation policy](../../../docs/validation.md) for the verification boundary; tests do not replace missing human evidence.

**Execution-contract revision:** The user explicitly requires evidence documents to remain untracked. The detailed report is local at `docs/verification/package-configuration-reload-evidence.md`, with raw artifacts under `target/configuration-package-integration/`; both are ignored by existing policy, not distributed or remotely retrievable. This replaces the retrievable-evidence requirement for this execution only. No `.gitignore` changes, force-add, staging or commits were made. Tracked documentation references this self-contained delivery update, not a missing local artifact link.

**Remaining human evidence:** Fresh integrated IME/preedit/candidate placement/focus cancellation; physical typing/Backspace/Enter and held-key rebind behavior; selection/copy after repeated font changes; complete nvim alternate exit and primary-history restoration; applicable backdrop/opaque/DPI observations and desktop OSC reset behavior. Prior T0001/T0002 user confirmations remain historical slice acceptance, not newly executed package checks. Owned smoke processes/configuration were cleaned up. The ticket is not marked Done while this evidence remains pending.

**Remote actions pending, not authorized or performed:** reconcile #172's published font exclusion with specs 0023/0024 and ADRs 0059–0061; align PR #201's delivered scope; add approved delivery/evidence links and reconcile related/sub-issue status on #155. The local-only report is not a remote evidence link. Do not close #155 until full scope is accepted or the user explicitly changes scope. Environment/directory remain included, not deferred.

## Out of Scope

New feature requirements, blanket interoperability claims, automatic approval of failed/skipped gates, unapproved Git ignore changes, or closing issues without execution authorization.
