# OSC 52 Windows Runtime Acceptance

## Current outcome: BLOCKED after latest-tree rebuild and rerun

This is an execution record for the controlled startup-isolation preflight, **not** an application/ConPTY clipboard acceptance pass. The actual Harbor process was not launched: the requested private child `USERPROFILE` does not isolate the current configuration loader. No real user configuration or clipboard contents were read, saved, logged, or modified in this blocked run.

The coordinator requested another run after the bounded raw-PTY draining fix. The latest tree was rebuilt and the harness rerun at `2026-10-01T03:42:30.8337907Z`; `crates/harbor-terminal/src/io.rs` then contained the bounded `sync_channel(PTY_QUEUE_CAPACITY)` transport. Startup isolation still failed, so application runtime acceptance remains blocked. Compilation and deterministic tests elsewhere do not establish runtime acceptance.

## Executed environment and snapshot

| Item | Observed value |
| --- | --- |
| Revision | `be6872fb144dce08f8de45d40b32add5beb0d194` |
| Tree | Dirty OSC 52 integration tree, with concurrent production-agent edits; no production edits by this harness agent |
| Latest preflight time | `2026-10-01T03:42:30.8337907Z` |
| Binary | `target/debug/harbor.exe`, dev profile |
| Binary SHA-256 | `C099240A09D3913E9EDCC91289524EDD3437C944867494ADF1429E94F39F35C3` |
| Binary write time | `2026-10-01T03:42:29.7459014Z` |
| Windows | `10.0.26300.0` |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Windows PowerShell runtime | `5.1.26100.9444` |
| `powershell.exe` file resource | `10.0.26100.8875 (WinBuild.160101.0800)` |
| `conhost.exe` file resource | `10.0.26100.8875 (WinBuild.160101.0800)` |
| System `conpty.dll` | Absent; inbox ConPTY API path, not a claimed separate DLL version |
| Intended named workload | Direct synthetic **Windows PowerShell OSC 52 probe**, `Console.Out.Write`/flush inside Harbor's ConPTY child |
| Actual workload | Private PowerShell known-folder isolation probe only; Harbor transport **not exercised** |

These file-resource versions describe the inspected Windows resources, not proof that a ConPTY session completed. The executable hash identifies a compiled snapshot, not a runtime pass.

## Reproduction

Run from the repository root in a controlled interactive Windows desktop:

```powershell
cargo build --bin harbor
powershell.exe -NoProfile -STA -File scripts/verify_osc52_windows.ps1 `
  -AcceptSyntheticClipboardReplacement `
  -ReportPath D:/Code/harbor/target/osc52-windows-runtime.json
```

Choose an existing writable location for `-ReportPath`; the example artifact is untracked under `target/`. The script emits only named outcomes, equality bits, text lengths, fixed content-free reasons, and version/snapshot metadata. It never emits protocol bytes, base64, rejected candidates, exception payloads, window titles/previews, or screenshots.

Exit codes: `0` means all recorded checks passed; `1` means a recorded failure; `2` means blocked or unrun required cases. The latest command exited **2**. Windows PowerShell syntax parsing returned `syntax_error_count=0`. Initial and latest-tree `cargo build --bin harbor` commands exited **0**, with an existing incremental-cache access warning. `python scripts/check_docs.py` and `python scripts/checklist_summary.py` both exited **0** after this evidence file was created. No workspace/clippy result is claimed by this record.

### Why startup isolation fails

Source inspection identifies the current chain:

1. `crates/harbor-config/src/lib.rs::default_config_path()` uses `dirs::home_dir()` plus `.harbor/config.toml`.
2. `Cargo.lock` selects `dirs 6.0.0` and `dirs-sys 0.5.0`.
3. `dirs 6.0.0` Windows `home_dir()` calls `dirs_sys::known_folder_profile()`.
4. That implementation calls `SHGetKnownFolderPath(FOLDERID_Profile)` with the current token, not `USERPROFILE` or `HOME`.

The harness starts a private PowerShell process with both `USERPROFILE` and `HOME` set to a unique temporary directory, invokes that same known-folder API, and records only whether the resolved path equals the child's private profile. The observed equality was **false**. It therefore stops before creating a Harbor process, before any clipboard seeding/read, and without inspecting or changing the real `~/.harbor/config.toml`.

**Coordinator action required:** authorize/provide an explicit isolated startup configuration seam, or an independently provisioned isolated Windows user/profile. Changing `USERPROFILE` alone cannot meet this test's safety contract. This is a runtime-test setup blocker, not evidence that OSC 52 parsing or host policy is defective. Tests do not authorize a production change.

The script's `-UserProfileIsolationVerified` switch is an explicit assertion for a future independently verified production path that honors the child `USERPROFILE`; it is **not a bypass to use with the current loader**. Do not use it merely to get past the preflight. The subsequent private emitter-ready sentinel detects configuration delivery, but does not justify knowingly launching against a real user configuration.

## Actual case outcomes

**Actual application clipboard passes: 0. Actual application clipboard failures: 0.** All 33 named application cases below were blocked at the safe-startup prerequisite; absence of execution is not a pass. The startup-isolation case itself records `BLOCKED`, equality `false`, with no clipboard length.

| Group | Cases represented by the harness | Latest result |
| --- | --- | --- |
| Default/explicit allow | Nonempty write and clear under omitted/default and explicit allow | BLOCKED |
| Explicit deny | Nonempty write and clear preserve a synthetic baseline | BLOCKED |
| Supported framing/subset | Empty selection, omitted padding, ST, deliberately split ST | BLOCKED |
| Confirm decisions | Approve/deny nonempty and clear, independent-window focus continuity | BLOCKED |
| First pending request | Additional synthetic flood must preserve original request and one dialog identity | BLOCKED |
| Focus/lifecycle | External-application cancellation, unfocused write, minimized write/clear | BLOCKED |
| Tabs | Inactive original tab after `Ctrl+t` creates a second emitter | BLOCKED |
| Tab-change/source-close stale | Separate active-tab transition and stale-source callback acceptance | BLOCKED; runner explicitly leaves these NOT RUN even after setup is unblocked |
| Invalid/read input | Malformed base64, unsupported selection, `?`, NUL, invalid UTF-8, cancellation, incomplete string | BLOCKED |
| Capacity | Exact decoded maximum, decoded maximum plus one, encoded cap plus one | BLOCKED |

## Controlled runner design and evidence limits

The pending application runner uses actual `harbor.exe` and per-launch private startup settings specifying `powershell.exe -NoProfile -File <this-script> -Emitter <control-directory>`. The child reads fixture-kind control files and writes OSC characters directly through `Console.Out.Write` and flushes. Neither shell command echo nor terminal keystrokes carry the raw protocol. Control files contain only fixture names and correlation IDs. Each child publishes a ready sentinel and a flush acknowledgement; acknowledgement proves child emission, **not** parser processing or clipboard success.

Every clipboard assertion seeds a synthetic baseline first. Clipboard reads hold the native clipboard lock and check that the clipboard owner belongs to this controller or this Harbor process before fetching Unicode text. Foreign/unknown owners are never read. Empty text can be observed by absence of Unicode text without fetching any contents. A controlled run intentionally replaces the clipboard and does not restore or retain unrelated previous contents. Do not interact with the clipboard while the runner is executing.

Foreground operations use discovered process-specific HWNDs and verified `GetForegroundWindow`; approval/denial use `y`/`n`, tab creation uses `Ctrl+t`. The external focus target is a private synthetic form owned by the runner. No unrelated Harbor process is stopped; only tracked test process objects are terminated, temporary private directories are removed, and the original foreground HWND is restored if still valid. App output/error streams are drained and discarded with tracing disabled; no terminal captures are retained.

Capacity fixtures are generated in memory, independently:

- Exact decoded maximum: **4,194,304 ASCII bytes**, **5,592,408 encoded bytes**.
- Decoded overflow: **4,194,305 ASCII bytes**, still **5,592,408 encoded bytes**; tests the decoded check independently of the encoded cap.
- Encoded overflow: **5,592,409 encoded field bytes**; tests framing retention rejection independently.

Small allowed fixtures include newline, tab, and Unicode; successful results compare exact strings, not prefixes. Negative cases compare exact baseline equality after a bounded dwell. Small-case dwell is one second after child flush; capacity cases use five seconds. These observations cannot prove absence of all future effects, high-water memory bounds, exact effect counts, or scheduling/coalescing invariants.

The full application portion has **not yet been validated by execution** because the current isolation prerequisite is false. Setup or foreground failures stop the runner with content-free classification and mark remaining cases NOT RUN. It does not turn absent dialogs, missing emitters, or clipboard-owner uncertainty into passes.

## Explicit exclusions and follow-up

NOT RUN in this evidence record:

- Remote editor, WSL, SSH, tmux, or representative editor-generated OSC 52 acceptance. The intended workload is a named direct synthetic Windows PowerShell probe only.
- Query reply/disclosure checks, rejected-byte visual suppression, preview/source-label correctness, and bounded preview rendering. Unchanged clipboard alone does not establish those outcomes.
- Allow latest-write coalescing, pending/raw-PTY retention high-water bounds, cancellation/reset text recovery, exact effect-once semantics, source closure/stale callback races, and tab-change cancellation.
- Injected native clipboard backend failure and content-free native failure diagnostics.
- Ordinary selection copy, bracketed paste, paste-confirmation, and cross-window paste regression acceptance.
- Windows 10, other Windows builds, native Unix, or different shell/application transports.
- Standard `cargo fmt`, all-target/all-feature clippy, workspace tests, and documentation checks **as runtime acceptance evidence**. Coordinator-owned deterministic gate results remain separate.

The latest-tree rebuild/rerun still records BLOCKED after the bounded raw-PTY change. Rebuild and rerun again if the coordinator changes the relevant binary. If startup isolation is unchanged, preserve the BLOCKED outcome. If an authorized safe seam is delivered, run the application cases, replace this pending record with observed equality/length outcomes and a new binary hash, diagnose actual failures without changing production code here, and separately close the remaining UI/transport/regression exclusions.
