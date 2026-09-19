# Validation Policy

## What This Document Means

This page defines required evidence, not a report that every gate has passed. [Current Status](current-status.md) records implementation scope; the [Roadmap](roadmap.md) selects release scope; the [Next-Stage Product Plan](next-stage-plan.md) defines upcoming acceptance outcomes.

Distinguish four things:

1. **Implementation exists** in source.
2. **Focused tests exist** for the behavior.
3. **A check was executed**, with a recorded revision, environment and result.
4. **The target application/platform was accepted**, with reproducible runtime evidence.

None automatically proves the next. A configured CI workflow is not a successful CI run. Future-feature acceptance below applies when that feature is delivered; it does not imply it exists today.

## Standard Quality Gates

Run at phase boundaries and before merging behavior changes:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

For documentation-only changes, run the two documentation scripts and inspect the diff, status claims and Markdown fragments. The link checker validates local file targets and language policy, not heading fragments, external websites, factual accuracy, or visual rendering. Run focused code tests if changing coverage claims requires fresh execution evidence; do not claim a workspace pass unless it was run.

Record environment limitations as **not run** or **blocked**, not as passing. Protocol counts are inventory statistics, not a release-readiness score.

## Evidence by Change Type

| Change                      | Required evidence                                                                                                      |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Parser state/transition     | One-shot and fragmented-input tests; malformed, cancellation, over-limit and recovery cases                            |
| Terminal model              | Focused cells, cursor, modes, margins, history, damage and reset assertions                                            |
| Terminal reply              | Exact bytes, bounded output, capability honesty; round-trip parsing where applicable                                   |
| OSC/DCS/APC effect          | Framing/cancellation, payload and decoded-size limits, permission and reset behavior                                   |
| Keyboard, mouse, focus, IME | Deterministic encoding/routing tests plus Windows runtime smoke                                                        |
| PTY lifecycle               | Spawn/read/write/resize, failure rollback, child exit, shutdown and leak-safe ownership                                |
| Configuration               | Loader fallback tests plus application/resource-update behavior; startup and live reload have different error policies |
| Tabs/panes/session routing  | Identity, independent geometry, focus/capture/IME, late output events, hidden-session progress and shutdown            |
| Renderer/text               | CPU geometry/model tests, practical GPU encode coverage, visual/runtime evidence and fallback checks                   |
| Performance change          | Before/after captures under the same scenario while correctness gates remain green                                     |
| Documentation               | Language/local links, changed fragments, source-backed claims, and plan/status consistency                             |

## Parser Safety

The safety boundary is `harbor_parser::Parser` plus its `VtHandler` sink. The parser-owned logical-retention contract covers CSI, pending UTF-8, OSC, and DCS/APC/PM/SOS framing. Handler-owned allocations need their own limits; the fuzz sink intentionally retains no callbacks.

The byte-at-a-time `Parser::advance` API has no chunk boundary. Chunked/one-shot equivalence is tested at `TerminalParser::put_bytes`, including its consumed-prefix `PutResult` and alternate-screen behavior.

### Stable Property and Regression Tests

```bash
cargo test -p harbor-parser
cargo test -p harbor-terminal
```

### Fuzz Replay and Campaign

The standalone harness and corpus are checked in. Runtime replay/campaign evidence on a supported libFuzzer host remains distinct from configuration and stable property tests. Windows setup alone is not a fuzz runtime result.

With nightly Rust and `cargo-fuzz`, run from `fuzz/` on Linux CI or another supported host:

```bash
cargo +nightly fuzz run parser -- -runs=0 -max_len=16384
cargo +nightly fuzz run parser -- -max_total_time=600 -timeout=5 -max_len=16384
```

The decoder treats inputs of at most 32 bytes as payload; longer inputs use the first 32 bytes as schedule and the rest as payload. Reproduce a seed from `fuzz/` with:

```bash
cargo +nightly fuzz run parser corpus/parser/utf8-fragmentation -- -runs=1
```

When invoked from the repository root, use the corresponding `fuzz/corpus/parser/utf8-fragmentation` artifact path. Minimize with `cargo +nightly fuzz cmin parser` or `cargo +nightly fuzz tmin parser <artifact>`.

Keep minimized inputs under `fuzz/corpus/parser/`. Every panic, stall, callback divergence or bound violation needs a named deterministic Rust regression. Unchecked arbitrary-input claims must not be promoted based only on a few fixed examples or an unexecuted workflow.

## Protocol Checklist Rules

[The checklist](protocol/checklist.md) owns detailed coverage:

- `[x]` requires a clear implementation plus focused tests or reproducible runtime evidence for the stated scope.
- `[ ]` means missing, partial, or not sufficiently verified; notes should distinguish these cases.
- Model/encoder completion does not imply a ConPTY/application path has passed runtime acceptance.
- Broad workspace success does not prove a particular protocol feature.
- Capability replies must not advertise unchecked or unsupported behavior.
- Summary sections must reference detailed coverage rather than maintain contradictory copies.

Calculate inventory with `python scripts/checklist_summary.py`; do not hand-maintain percentages in the README or roadmap.

## Windows Runtime Acceptance

Windows is the active product target. Changes to PTY, replies, input, rendering, window lifecycle or clipboard policy require Windows smoke evidence. Record Windows build and relevant application/ConPTY versions, including whether the application uses classic Console APIs or VT input.

### Baseline Application Matrix

| Workload                           | Observe                                                                                                       |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `cmd` and PowerShell               | Startup, prompt redraw, command execution, input, resize, exit and crash behavior                             |
| `nvim`, `less`, `fzf`              | Replies, redraw, alternate-screen transitions, keyboard modes, mouse/focus and search/navigation interactions |
| WSL/SSH and tmux where used        | The actual transport path and advertised capabilities; distinguish these from native Unix Harbor support      |
| Colored build output / large files | Throughput, sustained output, review-position stability, memory and responsiveness                            |
| Clipboard and selection            | Wide text, explicit versus soft newlines, bracketed paste, confirmation and focus restoration                 |
| IME                                | Preedit positioning, committed text, candidate-window placement, key suppression, focus loss and cancellation |
| Window lifecycle                   | DPI changes, minimize/restore, backdrop availability/fallback, presentation recovery and shutdown             |

Do not collapse the matrix to a single "terminal works" check. Specify applications and scenarios actually exercised, plus known exclusions.

Acrylic smoke includes Windows 10 client behavior and documented caption degradation from [spec 0008](../.grimoire/spec/0008-windows-acrylic-backdrop.md). Paste confirmation remains an opaque separate window. Native Unix acceptance is deferred under M6/N17 and does not block Windows-scoped milestones.

### Startup Settings and Keybindings

Copy `config.example.toml` to `~/.harbor/config.toml` and check:

- configured family/size and `pwsh.exe -NoLogo`;
- default/ANSI colors, cursor, selection and translucent background on the first frame;
- every documented default shortcut, selected/no-selection copy, bracketed/confirmed paste;
- main-screen history commands and alternate-screen pass-through; consumed chords must emit no terminal bytes;
- one replacement binding, multiple bindings for one command and empty-array unbinding;
- unknown command, malformed binding, duplicate/conflict and invalid binding value shape;
- one invalid scalar, one invalid color, invalid TOML and a missing/unreadable file.

Expected startup policy: keybinding failure restores the complete default binding table without discarding valid unrelated settings; scalar fallback is local; color failure restores the whole palette; document failure uses complete defaults. Repeat without Acrylic and confirm terminal background configuration does not change compositor tint.

### Next-Stage Acceptance Additions

These extend, rather than replace, the package-specific outcomes in [the product plan](next-stage-plan.md).

| Package                          | Additional evidence                                                                                                                       |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| N01/N02 — reflow and Unicode     | Repeated width/height changes, content/selection anchors, CJK/combining/emoji, styled blanks, alternate-screen restoration and eviction   |
| N03 — code pages                 | Reproducible 936/65001/legacy-code-page probes, Console A/W versus byte I/O, input/output, process-start timing and redirection           |
| N04 — live settings              | Last-valid-state retention, atomic-save/debounce, active/inactive session consistency, failed resource preparation and font-driven resize |
| N05/N06 — palette/profiles       | Availability and shortcut precedence, no leaked input, quoting/environment/directory validation and unchanged running sessions            |
| N07 — panes                      | Independent geometry, all-visible scheduling, focused routing, capture/IME/paste gate, hidden output and close/late-event races           |
| N08/N09 — search/shell workflows | Soft-wrap-aware results, reflow/eviction invalidation, command boundaries and safe directory fallback                                     |
| N10 — Kitty keyboard             | Negotiation and input traces in both directions through each claimed transport; repeat/release, layouts/AltGr and IME                     |
| N11/N12 — strings/graphics       | Exact replies, permission/decoded-memory bounds, image clipping/scroll/resize, reset/close reclamation and representative applications    |
| N13/N14 — glass UI               | Text contrast, opaque/high-contrast and reduced-effect fallback, sample-source constraints, DPI and before/after GPU/frame cost           |
| N15/N16 — capacity/release       | Multi-session resource budgets, reproducible performance, diagnostics, packaging and recorded dogfood sessions                            |

## Performance Evidence

Follow the [Profiling Guide](performance/profiling-guide.md). At minimum record:

- revision, executable profile and instrumentation;
- machine, OS, font configuration, viewport size and scale;
- workload, active/hidden sessions, history size and dwell time;
- before/after metrics and capture locations;
- throughput, frame/upload/atlas/presentation work, input/resize latency, idle CPU/GPU and memory where relevant.

Do not use DHAT-instrumented timing as startup/input-latency evidence. Keep historical captures immutable and make clear which revision they describe. Optimizations require comparable evidence, not just fewer lines of code or a renamed cache.

## Evidence Record Format

Store durable run records under `docs/verification/` when available, or link a retrievable CI artifact from the relevant work item. Do not invent a pass to fill a table.

```text
Revision / dirty-tree scope:
Environment / Windows and application versions:
Feature and exact claimed behavior:
Command or reproducible manual steps:
Expected result:
Observed result:
Outcome: PASS | FAIL | NOT RUN | BLOCKED
Artifacts / logs / screenshots:
Known exclusions and follow-up:
```

For UI/protocol recordings, avoid capturing secrets or unrelated terminal contents. A source-inspection record must identify itself as such and must not masquerade as an execution record.

## Windows Daily-Use Release Gate

Before calling a release daily-usable:

- identify included plan packages and release-critical exclusions;
- record standard checks and parser safety evidence;
- keep advertised capabilities consistent with implementation;
- document string/image/clipboard permission and resource limits;
- record representative Windows application, lifecycle and dogfood sessions;
- record current performance and diagnostic behavior;
- verify installation, configuration behavior and known limitations.

Optional full Kitty coverage, session persistence and liquid-glass refraction are not universal release blockers. Missing evidence for the features actually shipped remains a blocker regardless of visual polish.
