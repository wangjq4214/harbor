# Opt-in project capture integration

These scripts provide metadata for **Save current session as preset**. They do not
install themselves, edit profiles, recover history, or contact another machine.
Source them only in a shell where you deliberately want this integration. Closing
that shell removes its hooks. Existing configured Harbor environment overrides
remain a separate, default-off save choice; no runtime environment is captured.

## Setup

Local Windows PowerShell **7.3+**, in a disposable Harbor tab:

```powershell
. D:/Code/harbor/scripts/shell/harbor-project.ps1
```

The script preserves the existing prompt and PSReadLine history callback, and
refuses existing agent aliases/functions rather than replacing them. It installs
process-local functions for `pi`, `codex`, `claude`, and `opencode`. Use the direct
command names; explicit paths, arbitrary aliases/wrappers, scripts and nested
shells are not guaranteed recognition paths. No command text/history is retained.

Inside WSL bash, after making the script available there yourself:

```bash
source /path/to/harbor-project.bash wsl Ubuntu
```

Inside a **direct** SSH bash connection:

```bash
source /path/to/harbor-project.bash ssh user@host
# Or use an existing local SSH config alias that reproduces this connection:
source /path/to/harbor-project.bash ssh my-project-host
```

The supplied target identifies the environment to reopen. Use an SSH config alias
for custom connection settings such as ports/keys; do not put credentials or SSH
options in this field. Targets currently accept ASCII letters, digits, `.`, `_`,
`-`, and (SSH only) `@`, not option strings, IPv6 literals or shell fragments.
Verify that the target really reproduces the source environment before saving.
Harbor does not install this script remotely or test/probe the target.

Bash integration requires `od` and `tr` and a clean interactive shell without an
existing `DEBUG` trap, `PROMPT_COMMAND`, or `functrace`. It refuses competing hooks
instead of silently replacing them. If necessary, explicitly start a disposable
`bash --noprofile --norc` first. This may require setting your agent executable's
PATH in that disposable shell. Nested WSL/SSH combinations are not supported.

## Capture, review and relaunch

- Prompt metadata identifies the shell's current directory; command-start metadata
  invalidates it. A direct recognized agent invocation supplies its original argv
  and shell project directory while running. Compound commands/scripts may retain
  an invocation without asserting a current directory until the next prompt.
- After an agent exits, ordinary commands preserve its record. A later recognized
  invocation replaces it only for that terminal/environment. A reset clears records;
  there is no recovery of pre-integration history. Up to 16 environment records are
  retained per terminal; exceeding that bound clears older records.
- Save freezes a snapshot. Unknown/unusable metadata is **unknown**, never the
  initial cwd or OSC 7 fallback. Review permits explicit environment/directory
  correction. Changing environment clears the old environment's agent selection.
- Enter/Save… only advances naming. The in-window review has a separate final Save,
  Return and Cancel. Corrections are explicitly applied before final Save. Long
  review lines are selectable and horizontally navigable; the body scrolls and
  final actions remain visible. Return preserves corrections and the name.
- Included agent arguments need their own no-password/token confirmation. Suspected
  credentials hide the arguments and block saving that invocation. Explicitly omit
  the invocation or cancel; omission is visibly marked, not claimed as an exact copy.
  Detection is conservative and incomplete. Configured overrides require separate
  entry selection and acknowledgment; their values are never displayed.
- Project presets use fixed `pwsh`, `wsl.exe`, or `ssh.exe` launchers. Source launch
  program/arguments are reviewed as provenance, **not replayed as startup scripts**.
  The saved environment and directory are entered before the selected agent. Agent
  argument boundaries and existing resume flags are preserved; no resume/fresh
  option is added. PowerShell agent entry points must be native executables or
  PowerShell scripts; batch-only `.cmd`/`.bat` shims fail visibly rather than silently
  losing argument boundaries. Legacy non-project profiles are unchanged.
- Relaunched agents emit their reviewed metadata while running. After exit, prompt
  integration is needed again for current-directory recognition. These presets do
  not install/persist integration hooks. Missing metadata can always be corrected.
- No venv/Conda/Poetry activation, arbitrary command replay, runtime cloning or
  credential storage is provided.

## Protocol and verification boundary

The opt-in wire format is inert OSC `777` metadata:

```text
ESC ] 777 ; harbor ; 1 ; busy BEL
ESC ] 777 ; harbor ; 1 ; context ; HEX(kind) ; HEX(target) ; HEX(cwd) BEL
ESC ] 777 ; harbor ; 1 ; agent ; HEX(kind) ; HEX(target) ; HEX(cwd) ; HEX(agent) [ ; HEX(arg) ... ] BEL
```

`record` has the agent shape but leaves current context unknown. Fields are UTF-8
hex; an empty hex field is an empty argument. Payloads are bounded to 4000 bytes;
unsupported/oversized observations are unknown. NULs, invalid environments/paths
and unsupported commands are rejected. Metadata is untrusted: parsing performs no
filesystem/network operation and authorizes no save or launch. Arbitrary OSC 7 is
not this protocol. No raw payload/argument/environment-value diagnostics are emitted.

Focused tests: `python scripts/shell/test_project_integration.py` exercises isolated
pwsh and bash processes with synthetic agents (not an actual SSH connection).
Rust tests cover parser/scoping, mounted review, safe persistence/publication and
Host launch templates. An explicitly ignored ConPTY test exercises local pwsh and
four synthetic executable names. **Actual WSL/SSH connections, real agents, native
IME/window interaction and combined chooser/restart evidence remain T0004 work**;
these focused tests do not establish that end-to-end matrix.
