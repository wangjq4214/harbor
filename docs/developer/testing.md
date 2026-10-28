# Testing

Tests have two independent boundaries:

- **Ownership:** private unit tests live beside their module, under `src/<module>/tests.rs` or a topic directory. Public API and cross-crate contracts live in each crate's `tests/`.
- **Resources:** ordinary tests run by default. Resource-dependent cases live in `gpu`, `pty`, `native` or `perf` modules and carry `#[ignore]`. Select the module, not every ignored test.

Run commands from the repository root. Windows and installed system fonts are still required for Windows-specific font tests; “headless” means no native window, not a simulated font backend.

| Command | Runs | Requirements |
| --- | --- | --- |
| `just test` | Default workspace tests plus renderer-free terminal tests | Rust; no GPU, ConPTY child or desktop window |
| `just test-architecture` | Source ownership guards, also included in default tests | Rust; no native resources |
| `just script-test` | All Python tool tests, including profiling inputs | Python 3.12+ |
| `just sampler-test` | Process-sampler smoke test, not Harbor performance | Windows, PowerShell 7+ |
| `just test-gpu` | Required offscreen render/readback tests | GPU or DX12 WARP, system UI fonts |
| `just test-pty` | Real child-process/ConPTY integration, including GPU-backed reload transactions | Windows, bundled ConPTY, PowerShell 7+, GPU or WARP |
| `just test-native` | Explicit desktop, named-font and external-application acceptance | Read each test's requirements; interactive Windows, named fonts or Neovim |
| `just test-perf` | Release-mode diagnostics and measurements | Run serially without competing captures; not correctness gates |
| `just hmr-test` | Hot-reload feature tests | Application UI DLL built by the recipe |

`just qa` runs format/lint/default Rust checks. `just qa-all` additionally runs GPU, ConPTY, Python and HMR suites on Windows; it does not run desktop/manual acceptance or measurements. Windows CI runs default, core-only, required GPU, ConPTY, Python and sampler suites separately. Parser fuzz replay/campaigns remain in the separate [fuzz workflow](../../.github/workflows/parser-fuzz.yml).

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

[Terminal scripts](../../scripts/README.md#terminal-fixtures), including `scripts/terminal/manual_rendering.{sh,ps1,cmd}`, are **manual fixtures**, not Cargo tests or automatic E2E passes. Run them only inside a disposable Harbor tab. They can reset screen/mode state; clipboard probes require their own explicit consent.

See [widget testing](../widget/testing.md) for component-level boundaries and [history calibration](history-calibration.md) for measurement limitations. Generated evidence belongs under `target/` or an explicitly selected output path.
