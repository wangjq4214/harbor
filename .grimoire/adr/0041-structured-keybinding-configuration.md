# Structured Keybinding Configuration

**Status:** Proposed
**Date:** 2026-09-18

## Context

ADR-0040 represented each command override as an array of encoded chord strings under `[keybindings]`. That compact representation becomes ambiguous when bindings need separately identifiable modifiers and keys, especially when one command has multiple bindings.

## Decision

Represent each configurable command as a nested TOML table derived from its stable command ID. Each table contains a `bindings` array, and every binding is a self-contained record with a `modifiers` string array and one `key` string. Here, modifiers are keys such as `ctrl`, `shift`, or `alt`; they are not multi-stroke leader sequences.

```toml
[keybindings.app.new-tab]
bindings = [
  { modifiers = ["ctrl"], key = "t" }
]

[keybindings.terminal.copy]
bindings = [
  { modifiers = ["ctrl", "shift"], key = "c" },
  { modifiers = ["ctrl"], key = "insert" }
]
```

Omitting a command retains its registry defaults, while `bindings = []` removes its defaults. Validation and atomic fallback retain the behavior established by ADR-0040. This decision supersedes only ADR-0040's persisted command-to-chord-array representation; its command-dispatch, precedence, typing, and fallback decisions remain current.

The superseded command-to-string-array format is not backward compatible and must not receive a compatibility parser. Encountering it is an invalid keybinding shape: Harbor reports the error and restores the complete default keybinding set while preserving valid non-keybinding settings.

## Consequences

- Every binding carries its own modifiers and key, so multiple heterogeneous bindings have no array-pairing or Cartesian-product ambiguity.
- The format can add per-binding metadata later without changing the command table boundary.
- The parser, examples, tests, specification, and implementation must migrate from encoded chord strings to structured binding records.
- Multi-stroke key sequences remain unsupported and require a separate explicit contract extension.
