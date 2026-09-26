# T0001 Combining Text — Verification Record

## Scope and provenance

- Scope: T0001 of `.grimoire/ticket/0013-unicode-terminal-text-correctness/`; combining marks only. VS/ZWJ emoji presentation belongs to T0002; integrated Windows acceptance belongs to T0003.
- Candidate base revision: `0da3c3439400673e4921af8a15c5161787b40728`, dirty working tree. T0001 tracked diff SHA-256 (`git diff --binary -- . ':(exclude).grimoire/.gitignore' | sha256sum`): `3996f03a3356d24217245be54de235ce0c2e1492985a491112a61d476c99381d`. Untracked/ignored plan `0010-combining-text-end-to-end.md` and this verification document are excluded; an unrelated concurrent `.grimoire/.gitignore` edit is excluded. The fingerprint changed after moving `icu_properties` version management to workspace dependencies; `cargo check --locked -p harbor-terminal` passed, with no behavior change. The user's smoke-test binary revision was not supplied; this fingerprint identifies the current scoped source diff, not that binary.
- Automated evidence in `D:/workspaces/harbor`: `cargo test -p harbor-terminal --offline -q` PASS (896 unit + 48 integration); `cargo test --workspace` PASS (41 passing test-result groups, one existing ignored parser doctest); `cargo clippy --all-targets --all-features -- -D warnings` PASS; `cargo fmt --all -- --check` PASS; `python scripts/check_docs.py` PASS; `python scripts/checklist_summary.py` exit 0; `git diff --check` PASS. Workspace log: `/tmp/harbor-t0001-workspace-final.log`; clippy log: `/tmp/t0001-clippy-final.log` (local, not checked in).

## User-reported smoke test

- The user reported that the three-line `test_combining.ps1` fixture displayed without problems in Harbor and requested its removal; the root script was deleted afterward. This is **user-reported PASS** for that fixture only, not independently captured runtime evidence.
- The fixture emitted delayed `e` + U+0301 + `X`, line-start U+0301 + `X`, and ASCII + combined text + a wide character. The user subsequently explicitly accepted T0001 and reported that the remaining behavior has no issues. Exact Harbor build/revision, Windows/ConPTY/font/DPI versions, screenshots, clipboard codepoints and which extended resize/edit/eviction scenarios were run were not supplied. Acceptance by the user does not establish individual matrix executions.

## Reproducible Windows runtime procedure

1. Record Windows build, Harbor revision/dirty tree/profile/features, shell/ConPTY version, selected font and fallback fonts, point size, display scale/DPI and initial terminal columns/rows. Launch Harbor with a PowerShell shell. Keep screenshots/logs free of personal data.
2. From the shell, write a synthetic fixture using PowerShell (UTF-8 console encoding):
   ```powershell
   [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
   [Console]::Write("e$([char]0x0301)X`r`n")
   [Console]::Write("$([char]0x0301)X`r`n")
   [Console]::Write("12345678901234567890e$([char]0x0301)$([char]0x754C)`r`n")
   ```
   For a deliberate split, call the raw stream directly (with no intervening terminal control sequence):
   ```powershell
   $out = [Console]::OpenStandardOutput()
   $a = [Text.Encoding]::UTF8.GetBytes('e'); $out.Write($a, 0, $a.Length); $out.Flush()
   Start-Sleep -Milliseconds 250
   $b = [Text.Encoding]::UTF8.GetBytes([string][char]0x0301); $out.Write($b, 0, $b.Length); $out.Flush()
   ```
   Record the two writes and observed renderer update; a shell write boundary does not by itself prove distinct PTY read boundaries.
3. Inspect accent placement and cursor alignment against the following `X`, and inspect the dotted-circle cue at the isolated mark without altering selected text. Select/copy the first and second lines separately; inspect clipboard scalar codepoints (`[int[]][char[]](Get-Clipboard -Raw)`) and record expected `0065 0301 0058` and `0301 0058` (line terminators may be present in the copied selection).
4. Narrow and widen the primary terminal across the mixed ASCII/combining/CJK line. Select/copy before and after; check no duplicated or missing mark, CJK pair or generated padding. Repeat a recorded rows/columns sequence; test a long soft-wrapped line, an explicit newline, style/hyperlink and meaningful trailing blanks. Evict history past the configured capacity and verify selections whose endpoints were evicted invalidate.
5. Enter an alternate-screen application (record application/version, e.g. `nvim --version`), resize, and exit. Confirm rectangular alternate redraw and retained primary copy after returning. Exercise a previously rendered base followed by a delayed mark to check incremental repaint. Capture before/after screenshots and copied scalar sequences; note any missing-font or shaping fallback distinctly from retained text correctness.

## Outcomes

| Check | Expected | Observed | Status |
| --- | --- | --- | --- |
| Base + mark, one/split PTY reads | One advance; composed visual on base; raw `e` + U+0301 on copy | Delayed separate writes in smoke fixture reported fine; exact PTY read split and clipboard codepoints not recorded | NOT RUN |
| Isolated line-start mark | One cell and dotted-circle visual; raw U+0301 on copy | Smoke fixture reported fine; copied scalar sequence not recorded | NOT RUN |
| Edits, wide edge and dirty repaint | Whole units; no stale marks; repaint of changed base | Not executed | NOT RUN |
| Primary/alternate resize, selection and eviction | N01 geometry and surviving raw sequences preserved | Not executed | NOT RUN |
| Font/fallback and DPI change | Original text/width intact; observed visual outcome documented | Not executed | NOT RUN |

T0001 was marked Done at the user's explicit request, based on their acceptance and the automated evidence above. This is not a claim that every runtime matrix scenario was run or independently documented: screenshots, exact runtime versions, clipboard scalars and extended resize/edit/eviction details remain unavailable. Keep those matrix entries NOT RUN until scenario-specific evidence is supplied; T0003 still owns final integrated runtime evidence.
