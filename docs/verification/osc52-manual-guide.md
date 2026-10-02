# Manual-Assisted OSC 52 Testing

## Purpose and safety

[`scripts/test_osc52_manual.ps1`](../../scripts/test_osc52_manual.ps1) runs **inside an existing Harbor tab**. It emits synthetic protocol fixtures through that tab's PowerShell output and combines automatic clipboard equality checks with human UI/focus observations. It does not edit configuration, launch Harbor, automate focus/input, or bypass the separate [isolated runtime harness](osc52-windows-runtime.md).

Running the application cases intentionally **replaces the system clipboard**. Previous clipboard contents are neither read nor preserved. Save important contents yourself first; use a dedicated Windows test account if you do not want to affect your daily environment. Do not copy unrelated contents while testing. Native comparisons hold the clipboard lock, reject unknown/foreign owners before fetching data, and expose only equality and UTF-16 length. Reports contain no text, base64, preview, screenshots, window titles or backend exception messages. Clipboard-manager ownership changes may produce BLOCKED rather than a false pass.

## Prepare Harbor

1. Build from the repository root: `cargo build --bin harbor`.
2. Back up your existing `~/.harbor/config.toml` yourself. On Windows its location is `Join-Path ([Environment]::GetFolderPath('UserProfile')) '.harbor\config.toml'`.
3. Add or modify the existing clipboard table; do not duplicate the table or overwrite unrelated settings:

   ```toml
   [clipboard]
   osc52_write = "allow"
   ```

4. Fully exit and restart Harbor after each policy change. The script's `-Policy` is your declaration of the startup setting, not a configuration override or automatic verification.
5. Open Harbor, select the test tab, and close any existing clipboard/paste confirmation. Use another tab for the inactive-tab case, but do not close the emitting source tab. Start each case in the original foreground, non-minimized Harbor tab.

## Run inside Harbor

From a Harbor tab, change to the repository root. These commands start Windows PowerShell **inside that tab**, not a new Harbor instance:

```powershell
cd D:\Code\harbor
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy allow -Suite basic -AcceptSyntheticClipboardReplacement
```

Type `REPLACE` to confirm consent and that the declared policy matches the restarted application. For each case, follow the instructions printed **before emission**, operate any confirmation/focus transition yourself, then answer in the original tab:

- `y`: all described UI/focus behavior was observed.
- `n`: the described behavior was wrong.
- `s`: the visual/transition behavior was not verified; the case stays NOT RUN.

PASS requires both the expected clipboard equality and `y`. A clipboard equality alone does not prove that a confirmation appeared. Unknown ownership/unreadable data or unresolved native window state is BLOCKED. Do not approve the first-pending-flood case early: wait at least four seconds after its first dialog so the later requests have been emitted, then verify that the first request is still shown before approving.

Other suites:

```powershell
# No config change between these runs; give a unique report path on repeat runs.
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy allow -Suite negative -AcceptSyntheticClipboardReplacement
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy allow -Suite capacity -AcceptSyntheticClipboardReplacement
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy allow -Suite lifecycle -AcceptSyntheticClipboardReplacement
```

After changing the actual startup setting and restarting Harbor, repeat for `deny` and `confirm`:

```powershell
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy deny -Suite all -AcceptSyntheticClipboardReplacement
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy confirm -Suite all -AcceptSyntheticClipboardReplacement
```

Run those two commands in their respective application launches, **not** back-to-back against one policy. Background/minimized/inactive-tab cases give five seconds to change state; remain in it for at least three seconds after emission. Confirmation external-cancel/rapid-away-return cases require seeing the dialog before leaving, without approving it. Native focus/minimize preconditions are checked where observable; tab identity and visual transitions remain human-attested.

## Case groups

| Suite | Scope |
| --- | --- |
| `basic` | Allow/deny: text, clear, empty selection, unpadded encoding, BEL/ST and split ST. Confirm: approve/deny text and clear, first pending request preserved during 16 subsequent emissions. |
| `negative` | Malformed base64, unsupported selection, query, NUL, invalid UTF-8, cancellation and incomplete framing. Incomplete framing is cancelled after the observation dwell before ordinary prompts resume. |
| `capacity` | Exactly 4,194,304 decoded bytes, one decoded byte over (still within encoded limit), 5,592,409 encoded bytes. |
| `lifecycle` | Background, minimized, inactive tab; confirm additionally tests external cancellation and rapid departure/return. |
| `all` | All groups for the declared policy; not all specification acceptance criteria. |

## Reports and interpretation

Default report: `target/osc52-manual-<policy>-<suite>.json`. Existing reports are not overwritten. Supply a new path when repeating:

```powershell
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 `
  -Policy confirm -Suite basic -AcceptSyntheticClipboardReplacement `
  -ReportPath .\target\osc52-confirm-basic-run2.json
$LASTEXITCODE
```

Exit codes: `0` means all **selected** cases passed automatic equality plus human attestation; `1` means a selected case failed; `2` means blocked, unverified, interrupted, or report creation failed. Setup refusal before the runner starts exits nonzero and creates no report. Reports identify the binary hash, Windows/PowerShell versions, declared policy, selected scope and exclusions. Share the JSON and exact failing case/steps, not your real clipboard data.

Comparisons follow a two-second post-emission dwell and a five-second equality wait; they do not establish absence of every possible future effect. The script does not prove source-close/stale callback behavior, query reply/disclosure absence, process-wide memory bounds/responsiveness, injected native failures, ordinary copy/paste regressions or remote editor/WSL/SSH/tmux compatibility. Check those separately. A script PASS is not full product acceptance.

Restore your original configuration after testing and restart Harbor. The clipboard is left with synthetic test data; its previous contents were not saved.

## Script-only verification

This can run outside Harbor and does not access the clipboard, discover processes, construct native windows or emit OSC bytes to a real terminal:

```powershell
powershell.exe -NoProfile -STA -File .\scripts\test_osc52_manual.ps1 -SelfTest
```

It checks fixture round-trips/framing/capacity boundaries, outcome classification, and compiles the Windows native helper while testing its pure owner guard. Self-test evidence must not be confused with Harbor application execution. The earlier [automated implementation evidence](osc52-automated.md) and [blocked isolated-harness record](osc52-windows-runtime.md) retain their original scopes.

The self-test creates and removes only a private temporary directory for report-write checks. A read-only script review found no confirmed blockers; advisory recovery concerns led to best-effort CAN cancellation before exceptional output, independently guarded native cleanup, and atomic report publication without overwriting prior evidence. Application/native interruption behavior remains unverified.

### Executed script-only evidence

For the dirty script tree in this guide, Windows PowerShell 5.1 executed `-SelfTest`: **PASS, 28 checks**, including native helper compilation, pure owner guards and atomic report creation/collision/failure cleanup. PowerShell syntax parsing reported **0 errors**. Two external-process refusal checks passed: missing clipboard-replacement consent and execution outside Harbor both returned nonzero before the clipboard/protocol runner and created no report. The production report helper was also exercised with a synthetic extra payload field: that field was omitted, unknown equality/length stayed null, and human attestation was recorded. Documentation language/local-link checks, checklist calculation and `git diff --check` passed.

These checks did not launch Harbor or access the clipboard. Actual manual-assisted application cases have **not run** in this evidence; run them yourself as described above and retain their JSON reports.
