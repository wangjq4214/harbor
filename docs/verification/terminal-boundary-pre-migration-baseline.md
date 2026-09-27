# Terminal boundary pre-migration baseline (T0001 / T0005)

Collected 2026-09-27 09:05 UTC **before production boundary changes**, at `b055408a13c0e40e136d5d31e354562575d34638` (`refactor/separate`). At collection the tracked tree was clean; the only untracked files were the newly added measurement scaffold `crates/harbor-terminal/tests/session_baseline.rs` and the generated dependency-tree artifact in this directory. The benchmark is an ignored integration test; no production or pre-existing test source was altered for this baseline. Do not attribute these measurements to a later integration revision.

## Reproduce dependency graph

From repository root, with the revision above checked out and the lockfile unchanged:

```sh
cargo tree -p harbor-terminal -e normal --prefix depth > docs/verification/terminal-boundary-baseline-dependency-tree.txt
cargo tree -p harbor-terminal -e normal --prefix depth | grep -E '^[0-9]+(wgpu|arboard|winit|harbor-widget) v' | sort -u
```

The full **279-line** target-host normal-dependency graph is [terminal-boundary-baseline-dependency-tree.txt](terminal-boundary-baseline-dependency-tree.txt). Its depth-1 edges are `anyhow 1.0.104`, `bitvec 1.1.1`, `bytemuck 1.25.2`, `harbor-config 0.1.0`, `harbor-parser 0.1.0`, `harbor-pty 0.1.0`, `harbor-text 0.1.0`, `icu_properties 2.3.0`, `tracing 0.1.44`, `unicode-width 0.2.2`, `wgpu 30.0.0`. The forbidden-dependency query yields `1wgpu v30.0.0` and no `arboard`, `winit`, or `harbor-widget` in this *crate's normal graph*. This is **not a core-only target**: `wgpu` is a direct unconditional dependency and `Terminal` co-owns `TerminalRenderPipeline`, screen and PTY I/O. The whole rendered app has a different dependency graph; absence of the other three here does not assert their absence from the app. `cargo tree -e normal` excludes dev/build dependencies; repeat with `-e all` for the eventual core-only test target and inspect target-specific edges as needed.

## Reproduce CPU-side one-/multi-session workload

```sh
cargo test -p harbor-terminal --release --test session_baseline -- --ignored --nocapture
```

The scaffold constructs 1 or 8 separate **headless** 24x80 `Terminal`s, sends the same 66-byte colored status line with CRLF to each terminal 400 times in round-robin order, and for each update constructs a read-only `snapshot()` and calls `frame_demand(Instant::now())`. Seven timed samples per configuration follow one untimed warmup; session construction is inside each timed sample; median is sorted sample #4. Sessions execute serially on the calling thread, **not** concurrently. Each sample processes 400 or 3,200 terminal updates respectively. Throughput is aggregate updates per second, not frames per second. Input and test parameters are fixed in `crates/harbor-terminal/tests/session_baseline.rs` for a same-host before/after replay.

| Session count | Sorted sample durations (seconds) | Median (seconds/sample) | Aggregate updates/s |
| --- | --- | ---: | ---: |
| 1 | 0.245644, 0.2473254, 0.2526754, 0.2529374, 0.2600576, 0.2691465, 0.3708902 | 0.2529374 | 1,581.4 |
| 8 | 2.9946962, 2.9964548, 3.0108901, 3.0113089, 3.0152217, 3.017857, 3.0320869 | 3.0113089 | 1,062.7 |

Command status: PASS (1 ignored-by-default test explicitly selected and passed). `rustfmt --check --edition 2024 crates/harbor-terminal/tests/session_baseline.rs` checked after formatting. Not measured: PTY/ConPTY reader/writer, actual GPU renderer/upload/present, real windows/tabs, concurrency, resize, allocation profile, UI latency, or sustained real process output. These CPU-only proxy numbers must not be presented as full Windows application performance or a speedup claim. Final T0005 acceptance still needs comparable **post-T0004** numbers and separately documented Windows runtime/GPU scenarios.

## Environment

- Windows 11 Pro 10.0.26200 (build 26200; `cmd.exe /c ver` returned 10.0.26200.9457), x86_64-pc-windows-msvc; commands from MSYS2 MINGW64 bash on Windows.
- 13th Gen Intel Core i5-13600KF, 14 cores / 20 logical processors, 31.8 GiB physical RAM.
- GPU inventory: AMD Radeon RX 7900 XT and GameViewer Virtual Display Adapter. GPU not exercised by this probe; GPU driver version and ConPTY version not collected.
- `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1 (c980f4866 2026-06-30)`, release profile in repository (`opt-level=3`, `lto=true`, `codegen-units=1`).
- No claim of stable hardware frequency / exclusive host utilization. Compare distributions and environment, not isolated medians alone.
