# Expose OSC 7 as Metadata Without File-System Side Effects

**Status:** Proposed
**Date:** 2026-09-17

## Context

Shells emit OSC 7 `file://` URIs to describe their current working directory. Harbor needs this information at its terminal-to-host boundary, but interpreting untrusted terminal output as an instruction to change directories, read files, or access a network would cross the protocol side-effect boundary.

## Decision

Accept only `file://[host]/absolute-path`. Emit structured change/reset events through the existing terminal output-event boundary, store optional host/path metadata per terminal tab, and expose it through tab snapshots without adding UI. Hosts are ASCII and at most 255 bytes; paths are strictly percent-decoded UTF-8 and at most 2048 bytes. Reject userinfo, ports, queries, fragments, relative paths, malformed encoding, and control characters. Preserve URI path form without `PathBuf` conversion or normalization, and never perform implicit `chdir`, file reads, or network access.

## Consequences

- `TerminalOutputEvent` gains structured working-directory changed/reset variants; `TabManager` retains the latest valid value per tab and snapshots expose it for future host policy.
- Empty OSC 7 payloads and RIS clear the current value; invalid, cancelled, or over-limit payloads leave it unchanged.
- Raw OSC remains bounded by the parser's 4096-byte retention limit, with tighter decoded host/path limits at the terminal policy boundary.
- Windows drive paths remain URI paths such as `/C:/...`; platform conversion or normalization is outside this feature.
- Any future action or UI based on this metadata requires a separate explicit host policy.
