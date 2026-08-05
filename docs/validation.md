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

Windows is the active product target. Any change affecting PTY behavior, replies, rendering, input, window lifecycle, or clipboard policy requires a Windows smoke test.

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
