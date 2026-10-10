# Expose OSC 133 Shell-Integration Markers as Host Metadata

**Status:** Completed
**Date:** 2026-09-17

## Context

Modern shells emit OSC 133 FinalTerm-compatible escape sequences to mark prompt and command boundaries. Harbor needs awareness of these semantic markers at its host boundary to support future features (such as command navigation or block chrome) without executing shell hooks or injecting data back into the PTY.

## Decision

Parse and treat OSC 133 markers `A` (prompt start), `B` (prompt end / command start), `C` (command executed / output start), and `D` (command finished with optional integer exit code) as host metadata only. Safely consume and ignore unknown subcommands or malformed arguments without visible corruption, terminal state corruption, or PTY writes. Emit bounded `ShellIntegration` and `ShellIntegrationReset` events through `TerminalOutputEvent`, retain the latest marker in tab state, and reset deterministically on empty payloads and RIS (`ESC c`).

## Consequences

- `TerminalOutputEvent` gains `ShellIntegration(ShellIntegrationMarker)` and `ShellIntegrationReset` variants.
- `TabManager` retains the latest shell integration marker per tab and exposes it via `TabSnapshot::shell_integration()`.
- Unrecognized OSC 133 subcommands or malformed parameter payloads are discarded safely without side effects or crashes.
- No terminal replies or PTY byte injections are generated; marker handling is strictly unidirectional from terminal stream to host metadata.
- Any future command block chrome or navigation will consume this metadata via explicit host-level policies.
