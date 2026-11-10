# Named Profile Launch Contract Verification

## Outcome: automated and ConPTY suite PASS; interactive desktop acceptance tracked under T0004

This document records automated and Windows ConPTY execution evidence for ticket T0001 under spec 0052. It establishes that independent named session profiles, literal argument and environment semantics, local OSC 7 directory precedence, config-relative path resolution, atomic reload candidate rejection, strict launch failure, and session launch provenance are verified.

## Scope and execution environment

- Platform: Windows 10/11 x64, Microsoft ConPTY runtime 1.24.260710001.
- Test suites: `harbor-config`, `harbor-pty`, and `harbor` integration suites.
- Target contracts: Spec 0052 requirements R1, R2, R3, and R6 capture seam; acceptance criteria E2, E3, E4, E5, E6, E10.

## Verified contracts and observations

### 1. Independent profile schema and complete candidate acceptance (R1, R2, E6)

- Profile configuration is validated at startup and on reload under `[profiles.<id>]`.
- An invalid profile candidate rejects the entire configuration candidate, leaving currently accepted profiles, keybindings, fonts, and colors intact.
- Removing or editing a profile updates future session launches coherently; existing active sessions and their child processes are never modified or reconfigured.
- Verified by:
  - `crates/harbor-config/tests/profiles.rs`: `profile_settings_and_reload_acceptance`
  - `crates/harbor-config/src/profiles.rs`: table validation, unique name enforcement, ASCII identifier rules.

### 2. Argument and environment preservation (R1, R2, E3)

- Empty string arguments, arguments containing whitespace, quotes, backslashes, and Unicode characters are passed to the child process without loss or corruption.
- Environment overrides are child-only and literal; `$VAR` and `%VAR%` are not expanded.
- Case conflicts on Windows are rejected; `TERM` is protected (`xterm-256color`).
- Verified by:
  - `src/session_profile_tests.rs`: `named_profile_children_argv_environment_directory_reload_and_provenance`

### 3. Local named-launch directory precedence and OSC 7 policy (R3, E4, E5)

- Directory resolution follows strict precedence:
  1. Valid explicit launch directory.
  2. Configured profile directory (relative to configuration file's parent).
  3. Usable local OSC 7 directory from active source session.
  4. Inherited Harbor process directory.
- Configured relative directories resolve against the configuration file location, not Harbor's process working directory.
- Disappearing directories fail visibly and start no substitute shell.
- Remote authorities, UNC shares, and malformed/nonlocal OSC 7 paths are safely rejected without network probes.
- Verified by:
  - `src/profile_launch/tests.rs`: local URI translation, drive-letter conversion, percent-encoding handling, and remote rejection.
  - `src/session_profile_tests.rs`: `named_profile_children_argv_environment_directory_reload_and_provenance`.

### 4. Strict launch failure and immutable provenance (R1, R6, E2, E10)

- An invalid or missing executable fails visibly and starts no fallback shell, leaving existing tabs undisturbed.
- Each terminal session retains its immutable `Terminal::launch_provenance()` recording the exact executable, arguments, and initial directory used to spawn it.
- Verified by:
  - `src/session_profile_tests.rs`: `named_profile_children_argv_environment_directory_reload_and_provenance`.

## Scoped exclusions and deferred items

- Full interactive GUI chooser interaction is covered by T0002.
- In-window Quick Save preset creation and environment-aware project relaunch are covered by T0003 and T0004.
