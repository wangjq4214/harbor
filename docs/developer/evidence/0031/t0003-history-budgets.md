# T0003 — configurable dual-bounded history

**Result:** PASS for the ticket's functional implementation and standard gates. This is not T0004 resource/performance or full desktop acceptance, and does not close #166.

**Source:** [T0003](../../../../.grimoire/ticket/0019-bounded-history-and-performance-budgets/T0003-configurable-dual-bounded-history.md), [Spec 0031 R1/R2/R4](../../../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md), [ADR 0075](../../../../.grimoire/adr/0075-calibrated-history-and-text-unit-limits.md). The existing local, repository-ignored plan `.grimoire/plans/0042-bounded-history-budgets.md` was reused and refined; no separate planning/approval prerequisite was introduced.

## Source and environment

- Base revision: `63a34a3bce9b3e8f90a5dd82ae6bd3a9e069a0fc`; initially clean, tested with this implementation's uncommitted changes. No commit, issue mutation, user configuration change or external package installation performed.
- Windows NT `10.0.26300.0`, X64; Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`, host `x86_64-pc-windows-msvc`, LLVM `22.1.6`, Python `3.12.10`.
- Debug/test profile, workspace default features for tests; all targets/all features for Clippy. No DHAT, release benchmark, latency instrumentation or new memory measurement. Existing Windows/GPU/native tests ran as part of the workspace suite; their functional assertions are not resource measurements.
- Production scope: config settings/validation, normal-buffer retention/accounting, primary reflow, screen/alternate/saved-primary preparation and commit, terminal configuration transaction/construction, host policy/factory/acceptance. Regression scope: terminal history, configuration, read model, literal search, application search and host reload tests.
- Final tracked Rust diff SHA-256 (`git diff --binary -- '*.rs' | sha256sum`): `506244db4ff708014e3ca53451e9b5e6d6c4c4cb485c9e344cb4b8505b2af0dc`. New `crates/harbor-terminal/src/screen/history_tests.rs` Git blob: `5d16e4227fdf6561edc955ebb3055c89b3372f0e`. Documentation-only follow-up does not change behavioral evidence. The only Rust edit after the final full suite removed an unrelated blank line; formatting/diff checks were refreshed.

## Acceptance mapping

| Expected behavior | Observed evidence |
| --- | --- |
| Independent per-session row/source-byte caps, calibrated defaults/maxima, documented validation | `HistoryLimits`: defaults 4000 / 4194304, maxima 16000 / 16777216; config boundary/independent startup recovery and strict candidate tests pass. Zero is a zero allowance, never unlimited. Example and configuration guide updated. |
| Row-only, byte-only and combined overflow preserve newest whole physical rows; live text is excluded | `history_independent_row_byte_and_combined_eviction_keep_newest_whole_rows` covers distinct caps, zero allowances, too-large rows and newer live text. UTF-8 sequence/space test checks known byte lengths, wide continuations and unwritten padding. |
| Output, width/height reflow, saved primary, alternate transitions remain bounded | Resize/ring churn tests recalculate history bytes and assert both caps; saved-primary ConPTY test verifies immediate old-geometry shrink and deferred geometry prepared from the trimmed source, with no resurrection after growth/exit. Alternate output retains zero scrollback from entry as well as after rectangular resize. RIS retains accepted policy. |
| Logical truncation, stale anchors/selection/review/read/search targets | Head-truncation/review/churn test proves lost anchors stay unresolved across ring reuse and resize. Active/saved selection tests prove whole-selection clearing and stale read-range rejection. App history-reload test rejects old search targets and finds surviving live text. Existing read/search ring-reuse tests now explicitly request 1000 rows rather than depending on the old shipping default. |
| Coherent existing/hidden/saved-primary/future manual and watched reload | Host manual test exercises mixed font/palette/binding/launch/history acceptance, later-session preparation rejection, unchanged session identity, shrink, irreversible growth, alternate restoration and future factory policy. Real file-watch wake plus shared strict path exercises missing/malformed/invalid rejection and accepted shrink across existing sessions. |
| No rejected-budget eviction, including native compensation failure | Transaction test injects later-session native failure with both successful and failed compensation. Failed compensation is compared against ordinary geometry recovery under the accepted budget; the rejected zero budget is never installed. Dropped preparation preserves history; retry accepts both sessions. Existing host/native group rejection/resume tests remain green. |
| Documentation and honest scope | `config.example.toml` and `docs/app/configuration.md` describe units, scope, validation, reload/reflow, irreversible shrink, no unlimited mode and content bytes versus process/GPU memory. This report records functional gates only; T0004 and deferred visible-pane T0005 remain separate. |

## Design refinements and assessed findings

- Same-geometry budget changes use detached retention-only rebuilding, not reflow, preserving live cells/cursor/explicit tail spaces.
- Mixed geometry/budget reload pre-prepares compensation geometry with accepted limits before native effects. This intentionally adds temporary preparation overlap, not a process-memory promise.
- Saved ConPTY primary is trimmed immediately while producer geometry remains deferred; later deferred reflow cannot use pre-eviction source.
- Regression testing exposed historical tests that assumed the 1000-row default and alternate scrollback before the first resize. Fixtures now declare the old row allowance explicitly; alternate assertions now enforce the ticket's no-scrollback contract from entry. No unrelated product behavior was relaxed to obtain green tests.
- Inline risk review and acceptance comparison covered all changed production paths and new tests, including untracked test source. No confirmed blocking findings remain within T0003 scope.

## Commands and results

All commands ran from the repository root, serially. First full-suite attempts exposed the above outdated fixtures; after correction, the complete suite was rerun successfully, not reported as a baseline failure or selectively hidden.

| Command | Final observed result |
| --- | --- |
| `cargo test -p harbor-config --lib history_` | PASS, 2 focused tests |
| `cargo test -p harbor-terminal --lib history_` | PASS on the focused iteration; additional selection/recovery tests subsequently passed separately and in the complete suite |
| `cargo test --bin harbor history_` | PASS, 2 cross-session manual/watched tests |
| `cargo test -p harbor-terminal --lib configuration::tests::history_` | PASS, rejected-shrink/compensation coverage |
| `cargo test -p harbor-app --lib history_budget_reload` | PASS, consumer target invalidation |
| `cargo test --workspace` | PASS: host 142, app 178, config 21, terminal 1088; remaining workspace/integration/doc-test suites also passed. Terminal's 8 pre-existing opt-in tests stayed ignored; no ignored benchmark was counted as passing. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo fmt --check` | PASS |
| `python scripts/docs/check_docs.py` | PASS, refreshed after evidence/ledger edits |
| `git diff --check` | PASS |

Formatting was applied only to changed Rust files, with rustfmt output installed through anchored edits. Reproduction requires this dirty patch plus the new test file (or a later commit containing them), not the base revision alone.

## Explicitly not established

No release throughput/latency regression claim, process/private-byte bound, GPU/atlas residency or lifecycle-retirement acceptance is made. No new manual desktop matrix or simultaneously visible panes were exercised. Existing ignored/manual probes remain unexecuted here. T0004 owns integrated runtime/resource acceptance; T0005/#158 owns deferred visible-pane coverage. The earlier calibration measurements are not silently reused as changed-build performance evidence.
