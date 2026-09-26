# Content-Preserving Resize Windows Runtime Evidence

## Revision and Dirty-Tree Scope

- Candidate baseline: `05a4c62e34a7fee776704a3a9e74d0a0e9beed78` on `feat/reflow`, plus the T0007 source/test/workload changes listed in [the automation record](content-preserving-resize-automation.md).
- Unrelated untracked `TERMINAL_PLUGIN_ARCHITECTURE.md` is excluded from all claims.
- Final runtime execution must restate the exact commit or dirty-tree fingerprint. If source behavior changes, this record must be rerun rather than relabeled.

## Available Environment

- Windows build: `10.0.26200.9457`.
- Windows PowerShell: `5.1.26100.9444`.
- Neovim: `NVIM v0.12.5`.
- Deterministic workload: `scripts/resize_reflow_workload.ps1`.

Font family/size, display scale, initial viewport, Harbor build profile/features, shell executable/version, ConPTY context, and screenshot/log capture configuration were not recorded because no interactive session was executed.

## Required Scenario Matrix

| Scenario | Reproducible steps | Expected result | Observed result | Outcome |
| --- | --- | --- | --- | --- |
| Shell width-only resize | Run the fixture, narrow columns, inspect retained history/copy, widen to the original columns | Logical lines rewrap; CJK and meaningful blanks remain valid except documented eviction | Not executed | NOT RUN |
| Shell height-only resize | Change rows without changing columns, review history, then restore rows | Live bottom/cursor remain valid; review stays content-anchored where retained | Not executed | NOT RUN |
| Combined and repeated round trip | Repeat a declared rows/columns sequence and return to the original geometry while output continues | Every committed state remains coherent; retained logical copy is stable | Not executed | NOT RUN |
| Clipboard selection/copy | Select long colored/CJK text, explicit blank lines, ordinary trailing spaces, and styled blanks before/after resize | Copied plain text joins soft wraps, preserves hard breaks/meaningful blanks, and emits CJK once | Not executed | NOT RUN |
| Capacity eviction | Fill the fixed 1,000-row physical history budget, narrow enough to expand rows, and inspect a selection crossing evicted content | Oldest physical rows evict; a selection losing either endpoint invalidates instead of retargeting | Not executed | NOT RUN |
| Neovim alternate screen | Establish primary history/selection, launch `nvim`, resize, inspect redraw, exit | Alternate surface redraws at new geometry; resized primary state restores on exit | Not executed | NOT RUN |
| Practical failure/retry | Exercise a reproducible resize failure if a safe host procedure is available, then retry | Failed attempt does not partially commit; later retry succeeds | No safe interactive failure injection procedure executed | NOT RUN |

## Exact Workload and Resize Procedure

The runtime operator should:

1. Build the accepted candidate and record `git rev-parse HEAD`, `git status --short`, profile, features, and log filter.
2. Record Windows, shell, ConPTY/application, font, scale, and initial rows/columns.
3. Run:

   ```powershell
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/resize_reflow_workload.ps1 -Lines 1100 -PayloadWidth 96 -Seed 152170
   ```

4. Record exact dimensions for a width-only, height-only, combined, repeated, and original-geometry sequence rather than describing the window as merely “smaller” or “larger.”
5. Capture only synthetic fixture content. Redact prompts, paths, environment values, clipboard contents, and unrelated windows.
6. Repeat the declared primary checks around an `nvim` launch/resize/exit cycle and record `nvim --version` output.

## Expected and Observed Result

- Expected: the required shell, clipboard, review, eviction, and Neovim scenarios behave as specified above.
- Observed: no interactive Harbor window, ConPTY shell, clipboard, or Neovim session was executed in this agent run.
- Artifacts: none.
- Known exclusions: no tmux, WSL/SSH, native Unix, combining/variation-selector/ZWJ shaping, OSC 52, search, panes, or configurable history-capacity claim.
- Outcome: **NOT RUN**.

This missing runtime evidence blocks final T0007 acceptance and blocks superseding ADR-0018 or advancing ADR-0042 on acceptance grounds.
