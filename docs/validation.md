# Validation Policy

This document defines the evidence required before Harbor work is called complete. Individual plans link here instead of repeating the same quality gates.

## Standard Quality Gates

Run at each phase boundary and before merging behavior changes:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

A temporary environment limitation must be recorded in the change description; it is not evidence that a gate passed.

## Parser Safety Evidence

The parser safety boundary is `harbor_parser::Parser` plus its `VtHandler` sink. The
contract covers arbitrary bytes, progress, and parser-owned logical retention: CSI
storage, pending UTF-8 bytes, OSC bytes, and DCS/APC/PM/SOS bytes delivered through
the handler. Handler-owned allocation is not part of this bound; fuzzing uses a
non-retaining sink. The byte-at-a-time `Parser::advance` API does not expose chunk
boundaries, so one-shot/chunked equivalence is tested at
`harbor_terminal::parser::TerminalParser::put_bytes`, where the slice-ingestion call
and `PutResult` consumed-prefix/alternate-screen behavior are exercised separately.

Run stable property evidence on Windows or any stable Rust host:

```bash
cargo test -p harbor-parser
cargo test -p harbor-terminal
```

The standalone cargo-fuzz package, harness, and checked-in corpus are configured. The
fuzz decoder treats inputs of 32 bytes or fewer as all payload; longer inputs use the
first 32 bytes as schedule and the remainder as payload. Runtime replay/campaign
evidence is pending Linux CI. Windows setup alone is not a fuzz runtime result. With
nightly Rust and `cargo-fuzz` installed, run the replay and bounded campaign from
`fuzz/` in Linux CI (or another supported libFuzzer host):

```bash
cargo +nightly fuzz run parser -- -runs=0 -max_len=16384
cargo +nightly fuzz run parser -- -max_total_time=600 -timeout=5 -max_len=16384
```

A single seed can be reproduced with
`cargo +nightly fuzz run parser corpus/parser/utf8-fragmentation -- -runs=1` when run
from `fuzz/`; from the repository root, use
`cargo +nightly fuzz run parser fuzz/corpus/parser/utf8-fragmentation -- -runs=1`.
Minimize a discovered corpus with `cargo +nightly fuzz cmin parser`, or minimize one
crash with `cargo +nightly fuzz tmin parser <artifact>`, then keep the minimized input
in `fuzz/corpus/parser/`. Every panic, stall, callback divergence, or retention-bound
violation must also become a named deterministic Rust regression before the
corresponding checklist claim is marked complete. Until Linux CI records runtime
replay/campaign evidence, arbitrary-input checklist claims remain unchecked.

## Evidence by Change Type

| Change                          | Required evidence                                                                          |
| ------------------------------- | ------------------------------------------------------------------------------------------ |
| Parser state or transition      | Focused one-shot and fragmented-input tests; malformed, cancellation, and recovery cases   |
| Terminal model behavior         | Focused screen-state assertions covering cursor, cells, modes, margins, and reset behavior |
| Terminal reply                  | Exact byte-format test, length bound, and round-trip parsing where applicable              |
| OSC/DCS or external side effect | Boundary, cancellation, permission, and payload-limit tests                                |
| Keyboard, mouse, focus, or IME  | Deterministic encoder/routing tests plus a Windows runtime smoke test                      |
| PTY lifecycle                   | Spawn, read, write, resize, child exit, shutdown, and leak-safe failure behavior           |
| Renderer behavior               | CPU-side geometry tests, GPU encode coverage where practical, and visual/runtime evidence  |
| Performance optimization        | Before/after capture under the same scenario; correctness gates remain green               |
| Documentation-only change       | Local-link and language-policy checks; code tests are optional unless claims changed       |

## Protocol Checklist Rules

[`protocol/checklist.md`](protocol/checklist.md) is the feature-coverage source of truth.

- `[x]` means a clear implementation exists and is backed by focused tests or reproducible runtime evidence.
- `[ ]` means missing, partial, or insufficiently verified.
- A roadmap checkbox is not protocol evidence.
- Broad workspace test success does not prove a specific protocol behavior.
- Claims about arbitrary input require fuzz or property evidence, not a few fixed byte samples.

Calculate current totals with:

```bash
python scripts/checklist_summary.py
```

## Runtime Acceptance

Windows is the active product target. Any change affecting PTY behavior, replies, rendering, input, window lifecycle, or clipboard policy requires a Windows smoke test. Window-lifecycle smoke for Acrylic includes Windows 10 client acrylic and documents caption degradation per [spec 0008](../.grimoire/spec/0008-windows-acrylic-backdrop.md) E2E; paste confirmation opacity is unchanged.

The Windows daily-use gate requires recorded sessions with representative workloads such as:

- shell startup, command execution, resize, and exit
- `nvim`, `less`, `fzf`, and colored build output
- alternate screen transitions
- heavy scrollback and sustained output
- selection, clipboard, bracketed paste, and paste confirmation

Unix runtime acceptance belongs to roadmap phase P8 and does not block earlier Windows phases.

## Performance Evidence

Use the procedures in [`performance/profiling-guide.md`](performance/profiling-guide.md). Record at minimum:

- commit and executable profile
- machine and OS
- font configuration
- viewport dimensions
- workload and dwell time
- profiling mode
- before/after metrics

Do not use instrumented DHAT timing as startup-latency evidence.

## Release Evidence

Before a release is called daily-usable:

- all release-critical checklist exclusions are documented;
- parser fuzz/property gates pass;
- replies advertise only implemented capabilities;
- string and external-effect paths have explicit limits and permission behavior;
- benchmark and dogfood results are recorded;
- crashes and fatal GPU/PTY errors produce actionable logs.
