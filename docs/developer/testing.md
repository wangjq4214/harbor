# Testing

Tests have two independent boundaries:

- **Ownership:** private unit tests live beside their module, under `src/<module>/tests.rs` or a topic directory. Public API and cross-crate contracts live in each crate's `tests/`.
- **Resources:** ordinary tests run by default. Resource-dependent cases live in `gpu`, `pty`, `native` or `perf` modules and carry `#[ignore]`. Select the module, not every ignored test.

On Windows, the `just` test recipes use PowerShell 7 and require the locally
restored ConPTY source (`./scripts/setup/fetch_conpty.ps1`). For direct use:

```powershell
./scripts/dev/test_cargo.ps1 --locked -p harbor-pty filter -- --ignored --test-threads=1
```

The wrapper runs `cargo test --no-run` with the same Cargo selections, features,
target and filter, stages the complete runtime beside every reported executable
(including fresh shared-cache artifacts), then runs the original `cargo test`
with its harness arguments. `-CargoPath` selects a Cargo executable. It rejects
`--doc`, `--no-run` and custom `--message-format`: rustdoc-generated executables
are not reported as Cargo artifacts, and those modes cannot use this contract.
Ordinary `cargo test` may also run doctests; their transient executables are not
staged by this wrapper and must not rely on an adjacent ConPTY bundle. The two
Cargo invocations assume artifacts are not concurrently relocated or replaced
between staging and execution. CI's direct upstream Cargo commands and
non-Windows recipes are unchanged.

The Windows wrapper deliberately sets `CARGO_SHARE_WORKSPACE_UNITS=0` for both
Cargo invocations and their children (including trybuild): the huacnlee/cargo
fork's shared workspace units break nested trybuild builds. This also overrides
an explicit caller value during the tests and restores it afterward. Upstream
Cargo ignores the fork-specific variable; builds and direct Cargo commands are
not changed.

Run commands from the repository root. Windows and installed system fonts are still required for Windows-specific font tests; “headless” means no native window, not a simulated font backend.

| Command | Runs | Requirements |
| --- | --- | --- |
| `just test` | Default workspace tests plus renderer-free terminal tests | Rust; on Windows, the configuration watcher test also uses a GPU fixture and ConPTY child |
| `just test-architecture` | Source ownership guards, also included in default tests | Rust; no native resources |
| `just script-test` | All Python tool tests, including profiling inputs | Python 3.12+ |
| `just sampler-test` | Process-sampler smoke test, not Harbor performance | Windows, PowerShell 7+ |
| `just test-gpu` | Required offscreen render/readback tests | GPU or DX12 WARP, system UI fonts |
| `just test-pty` | Real child-process/ConPTY integration, including GPU-backed reload transactions | Windows, bundled ConPTY, PowerShell 7+, GPU or WARP |
| `just test-native` | Explicit desktop, named-font and external-application acceptance | Read each test's requirements; interactive Windows, named fonts or Neovim |
| `just test-perf` | Release-mode diagnostics and measurements | Run serially without competing captures; not correctness gates |
| `just hmr-test` | Hot-reload feature tests | Application UI DLL built by the recipe |

`just qa` runs format/lint/default Rust checks. `just qa-all` additionally runs GPU, ConPTY, Python and HMR suites on Windows; it does not run desktop/manual acceptance or measurements. Windows CI runs default, core-only, required GPU, ConPTY, Python and sampler suites separately. Parser fuzz replay/campaigns remain in the separate [fuzz workflow](../../.github/workflows/parser-fuzz.yml).

## Pre-merge quality workflow

The [Windows quality workflow](../../.github/workflows/windows-quality-gates.yml) also runs as a reusable workflow for tag releases. Every PR to main runs every lane; no path filter bypasses validation.

| Job | Responsibility | Time limit |
| --- | --- | --- |
| Quick checks | Markdown consistency, Python tool tests, Windows sampler and development-wrapper tests, workspace formatting and package version/tag validation | 10 minutes |
| Rust validation | Strict Clippy, default/core-only tests, serial WARP GPU/ConPTY suites, compiled guide examples and warning-free API reference | 45 minutes |
| Release packages | Static-CRT x64 build, matching MSI/ZIP validation, rejection tests and checked artifact upload | 45 minutes |
| `quality` | Aggregate all three required lanes, writing their results to the job summary | 5 minutes |

Quick checks must pass before Rust validation and release packaging start in parallel. Rust commands remain on one runner to reuse compilation; GPU and PTY tests remain serial. The two build lanes have separate dependency-cache identities: debug validation versus static-CRT release. Installed tool binaries are not cached, workspace build products are excluded by rust-cache's default dependency-only policy, and failed jobs do not save caches. Checked packages remain artifacts, not reusable cache inputs.

Keep the required status named **`quality`**. Its job runs even when a dependency fails or is skipped, and [the aggregate gate](../../scripts/ci/check_quality_gate.py) accepts only explicit success from all required lanes. A missing, failed, cancelled or skipped lane cannot pass. The main-branch ruleset requires this status; splitting jobs does not require replacing that rule.

New commits cancel superseded runs for the same PR in both the quality and fuzz workflows. Other event types do not cancel in-progress runs, and the reusable quality workflow uses a different concurrency group from its release caller. Fuzz replay and campaigns retain their separate triggers and have 20/25-minute job limits.

For transient failures, use Actions' **Re-run failed jobs** so successful lanes can be reused; re-running the aggregate alone does not rerun an unsuccessful dependency. A green package job or downloadable PR artifact is not an overall quality pass. The [release workflow](../../.github/workflows/windows-release.yml) waits for the entire reusable workflow before publishing the checked MSI and ZIP.

This structure targets earlier cheap-error feedback and less serial waiting, not fewer checks. Compare cold and warm hosted runs, including cache restore/save and runner queue time, before claiming a wall-time or cost improvement. It does not add automatic desktop/manual acceptance or performance gates.

## Advisory PR performance report

The [measurement workflow](../../.github/workflows/pr-performance.yml) runs separately
on every PR to main. It is **not** a dependency of `quality`, and performance changes
never block merging. Keep the existing required status/rules unchanged; do not add
the performance jobs as required checks.

The report compares the event's exact base SHA with its candidate merge SHA, not
two different hosted runners or CI job durations. One shared
[public-API probe](../../scripts/ci/performance_probe.rs) is copied unchanged into
both checkouts, compiled sequentially in release/default-feature mode, and the
executables are staged separately. Compilation finishes before timing begins.
Four processes per revision per stage alternate A/B then B/A order, with warmups excluded.

The fixed workloads cover ASCII/ANSI/Unicode ingestion (2000 rows), clean/local
engine updates with one/eight sessions, primary narrowing/widening with populated
history, alternate resize, and ordinary/emoji text preparation on DX12 WARP.
They report CPU elapsed time, not native input/output-to-present, ConPTY resize
latency, physical-GPU performance, memory residency or idle power. The probe is
not the packaged static-CRT application. Update measurements are batch-average
times per update. Text-preparation timing excludes GPU completion.

The comment shows base/head/merge SHAs, median and p95 times, signed percentage
changes, environment, probe fingerprint and between-process median ranges.
Medians/p95s are computed per process and then summarized across processes.
Positive changes mean slower; negative changes mean faster. Hosted-runner noise
is not calibrated, so small differences are not confirmed regressions. No numeric
regression threshold is applied. Missing, invalid, interrupted or failed
measurements are explicitly unavailable, never a zero-change or passing result;
a failed prepare stage can coexist with completed core results.

A [trusted publisher](../../.github/workflows/pr-performance-comment.yml) maintains
one GitHub Actions bot comment per PR, creating it once and updating it on later
runs/reruns. It verifies the source run, PR association, current base/head/merge
SHAs and run attempt. Superseded attempts, stale commits and older results cannot
replace a newer published report. Use **Re-run all jobs** on **PR performance**
to repeat measurements; rerunning only the publisher does not remeasure.

The publisher uses `workflow_run` and default-branch code only. The measurement
workflow runs PR code with a read-only token and no persisted checkout
credentials. The write-token publisher never executes PR code or artifact
scripts/Markdown: it validates a bounded JSON payload and regenerates the comment
from known case names and numeric samples. Fork runs use the same permission
separation, subject to GitHub's normal approval policy for fork workflows.
Publication failure does not affect `quality`; inspect the publisher logs if
repository permissions prevent commenting.

No downloadable report is required. A one-day, attempt-specific internal JSON
artifact carries data between the two permission boundaries; users read the PR
comment or measurement job summary. Missing/invalid artifacts produce an
unavailable notice when the run can still be associated with the current PR.
**Rollout:** the publisher workflow and its scripts must first exist on the
default branch before `workflow_run` can publish comments. Local tests do not
establish that GitHub has actually posted a comment.

## GPU selection

All offscreen suites share [one test-only device fixture](../../tests/support/gpu.rs). An explicitly selected GPU test **fails** if its adapter/device is unavailable; it never returns early and counts as a pass.

PowerShell:

```powershell
$env:HARBOR_TEST_GPU = 'dx12-warp'
just test-gpu
just test-pty
Remove-Item Env:HARBOR_TEST_GPU
```

Values are `auto` (default), `dx12`, `dx12-warp` and `vulkan`. `HARBOR_TEST_BACKEND` remains a fallback alias for existing local commands; `HARBOR_TEST_GPU` takes precedence. Native host tests exercise the production host/backend path rather than substituting the offscreen fixture.

To select a case, keep its resource namespace, for example `just test-native native::live_nvim_async_resize_keeps_alternate_app_responsive`. Do not run an unfiltered workspace `--ignored`: it mixes desktop tests, fonts, applications and release-only measurements.

## Navigation and fixtures

- [Terminal facade suites](../../crates/harbor-terminal/src/terminal_tests.rs): text, resize, SGR, metadata, alternate screen, input, selection, scheduling and session I/O.
- [Screen suites](../../crates/harbor-terminal/src/screen/tests.rs): editing, wide cells, margins, wrapping, damage and buffer/state contracts.
- [Workspace suites](../../crates/harbor-app/src/workspace_tests.rs): tabs, search, notifications, palette and popup siblings. Palette fixtures are shared explicitly, not borrowed from another test case.
- [Quad renderer suites](../../crates/harbor-widget/tests/quad_renderer.rs): helpers plus GPU topics for gradients, shadows, clips, icons, external surfaces and backdrop materials.
- Macro grammar/compile-fail cases remain in `crates/harbor-widget-macros/tests/ui/{pass,fail}`, separate from runtime tests.
- Keep helpers local to the smallest shared owner. Source ownership guards belong in an `architecture` suite; they inspect the production file directly, never split source on a test-module declaration. Do not expose production internals just to move a test, or add a generic test framework.
- The public terminal suites share a [parser feed](../../crates/harbor-terminal/tests/support/parser_feed.rs) that checks progress and applies alternate-screen transitions before the unconsumed suffix. Chunk-equivalence cases must exercise the real slice-ingestion boundary, not just regroup identical byte-at-a-time calls.
- Mounted control tests share [input shorthand](../../crates/harbor-widget/tests/support/control_input.rs) for left-button pointer 0 and keyboard events. Dispatch does **not** update the runtime; tests retain explicit update timing and use their own helpers when IDs, modifiers or clocks differ. Routing suites instead share [event constructors](../../crates/harbor-widget/tests/support/input_events.rs) that preserve caller-supplied buttons and pointer IDs without dispatching.
- Offscreen suites share [texture readback](../../tests/support/readback.rs), while each suite retains its rendering and assertions. Readback explicitly selects tightly packed or padded rows and preserves RGBA/BGRA and linear/sRGB bytes without conversion.
- Parameterized cases retain named scenarios and useful failure diagnostics. Keep independent contract boundaries, invalid-input matrices and real resource tests; fewer test functions alone are not a simplification metric.

[Terminal scripts](../../scripts/README.md#terminal-fixtures), including `scripts/terminal/manual_rendering.{sh,ps1,cmd}`, are **manual fixtures**, not Cargo tests or automatic E2E passes. Run them only inside a disposable Harbor tab. They can reset screen/mode state; clipboard probes require their own explicit consent.

See [widget testing](../widget/testing.md) for component-level boundaries and [history calibration](history-calibration.md) for measurement limitations. Generated evidence belongs under `target/` or an explicitly selected output path.
