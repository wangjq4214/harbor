# OSC 52 Automated Verification

## Outcome: automated gates PASS; application acceptance BLOCKED

This records deterministic execution and source-review evidence for spec 0017, not an OS clipboard or ConPTY application acceptance pass. The separate [Windows runtime record](osc52-windows-runtime.md) reports the safe startup-isolation blocker and zero application clipboard passes. No real user configuration or clipboard was accessed by these automated checks.

## Snapshot and environment

- Base revision: `be6872fb144dce08f8de45d40b32add5beb0d194`, plus dirty OSC 52 implementation and test tree; no commit created.
- Windows build: `10.0.26300.0` (runtime preflight environment); repository `D:/Code/harbor`.
- `rustc 1.97.1 (8bab26f4f 2026-07-14)`; `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Final tests ran after PTY attachment, EOF, normal/resize streaming, foreground-monitor and actual confirmation-root callback fixes.
- Final security-seam source SHA-256 values:

| File | SHA-256 |
| --- | --- |
| `src/clipboard.rs` | `097cce2786335654b64d5dac6e7e2d1eaac4479aa1765cd1fdfdcd1e4a096f5b` |
| `src/clipboard/foreground_monitor.rs` | `7d95306f9c2753bfc61de5171bfb37cf373dcc6eeccc3d31b304d78c6d71df44` |
| `src/shell.rs` | `33aa9cd276aba4374b2d61c6a7f9d2ad985473718ecf2e2f709578ba39fde4cf` |
| `crates/harbor-terminal/src/io.rs` | `2d431efe1d45e258014907829d1aaffd370be6ecd9828056a4882b89df39bbaa` |
| `crates/harbor-terminal/src/lib.rs` | `197cf8222b57ef7a97a078fdb711d804fa8638ecb3d31a75f4d748881735692d` |

These identify reviewed source, not a runtime executable hash. The older binary hash in the preflight record must not be treated as this final source snapshot.

Final dev build completed at `2026-10-01T04:12:13Z`: `cargo build --bin harbor` PASS. The resulting **unlaunched** `target/debug/harbor.exe` SHA-256 is `8680522877103015078b2d2cad4e8e276237778f344b85f7373b5d34acb5b590`. Windows PowerShell parser verification of the runtime harness returned `syntax_error_count=0`; this is script syntax evidence only, not application execution.

## Executed gates

| Command | Observed outcome |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 2,255 passed, 0 failed, 5 ignored across 43 suite records; root binary 105 passed; app library 92 passed |
| `python scripts/check_docs.py` | PASS: language and local links |
| `python scripts/checklist_summary.py` | PASS: inventory calculated; counts are not acceptance evidence |
| `git diff --check` | PASS |
| `cargo build --bin harbor` | PASS: dev binary built, not launched |
| Windows PowerShell harness syntax parse | PASS: 0 syntax errors; application portion not run |

Local untracked command logs are `target/osc52-final-{fmt,clippy,workspace,docs,checklist,diff}.log`. An earlier final-gate attempt failed Clippy because the foreground monitor placed production items after its test module; moving the tests to the end fixed it, and Clippy plus the workspace tests were rerun successfully. Existing incremental-cache access warnings did not fail the final commands.

## Requirement-to-evidence

| Requirement | Evidence and qualification |
| --- | --- |
| R1: exact write subset | Parser/terminal tests and contract tests cover BEL/ST fragmentation, exact selections, strict padded/unpadded standard base64, canonical tail bits, UTF-8/NUL, clear, reads denied, cancellation/reset and recovery. No runtime transport acceptance inferred. |
| R2: fixed capacity/bounded delivery | Encoded 5,592,408 and decoded 4,194,304 byte caps are independently tested, including decoded overflow that fits the encoded cap. First/latest/discard mode is preserved across PTY attachment. Normal and resize-barrier paths stream bytes instead of collecting a concurrently refilling raw queue into a vector. Deterministic refill tests cover queue high-water and at most one pending clipboard event, metadata ordering and old-geometry replies. Native sustained-output responsiveness/process-wide memory measurement remains unrun. |
| R3: policy/source/foreground | Startup loader default/valid/invalid/local fallback tests; session-qualified stable identity and active/live checks; permission Boolean matrix; EOF/finished-reader tests; host rechecks at execution. Native foreground scheduling and minimize transitions require runtime evidence. |
| R4: immutable per-request confirmation | Policy tests cover one pending request, rejection of additional requests, exact single-use approval, stale eligibility and no permanent grant. Presentation tests cover source/size and 1,024-character preview. The real clipboard confirmation root's deny and allow callbacks are exercised through Runtime focus/Tab/Enter dispatch. External-departure epoch tests include departure/return and fail-closed registration. Native window lifecycle ordering, source-close/tab-change late outcomes, foreground-hook delivery and DPI presentation remain runtime needs-verification. |
| R5: native effects/diagnostics/honesty | Recording/failing sinks verify exact approved and empty writes, propagated failure and content-free backend errors. Parser and session-qualified request Debug exclude payloads; read queries emit no replies; capabilities stay unchanged. Ordinary paste deterministic regressions pass, but actual native copy/paste acceptance remains unrun. |

Source review identified and drove the attachment-policy reset, raw-queue retention and scheduling-dependent EOF-test fixes. Automated evidence is not a claim that all specification acceptance criteria are closed. Windows policy/capacity cases, native clipboard failure, copy/paste regressions and named application/WSL/SSH/tmux compatibility remain blocked or not run as recorded separately.

## Final read-only security-seam review

No evidence-backed blocker was found in the final attachment, streaming resize, authorization/lifecycle and foreground-monitor changes. The reviewer read both root clipboard files completely, all of `src/shell.rs`, and the streaming I/O implementation/tests against spec 0017 and ADRs 0049/0050. This is source-review evidence; that review executed no additional build, native UI or clipboard tests.

Queued candidates are authorized using their stable source's current eligibility at admission and execution, as specified; emission-time focus provenance is not tracked. Consumed denied requests are not revived by later activation. Already-admitted confirmations are separately cancelled on invalidating transitions.

**Needs-verification:** WinEvent departure delivery is asynchronous. The epoch check prevents revival once a departure is delivered, but static inspection and the arithmetic test do not prove that a rapid external away/return notification arrives before a queued approval, including nested native dispatch. Actual foreground checks reject a still-external application; an undelivered historical departure remains a runtime ordering question. Native registration/unhook, callback latency, confirmation/minimize transitions and ConPTY interrupt/reply/wake behavior remain unexercised. Overall runtime acceptance stays BLOCKED.
