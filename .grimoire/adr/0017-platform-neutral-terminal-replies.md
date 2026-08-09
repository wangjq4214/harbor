# ADR-0017: Platform-neutral terminal replies via Screen buffer

**Status:** Completed
**Date:** 2026-08-06

## Context

Harbor can write keyboard and paste input to the PTY, but lacks a terminal-to-PTY reply path for VT query sequences (DSR, CPR, etc.). We need a platform-neutral communication boundary that allows the parser and screen layers to queue query responses without direct dependencies on OS-specific ConPTY/PTY writer types.

## Decision

We chose to store pending outgoing VT replies inside a simple, platform-neutral `replies: Vec<u8>` buffer owned directly by the `Screen` struct (Option 1B). The `ScreenHandler` appends protocol bytes to this buffer during CSI dispatch, and the async terminal worker drains and writes them to the PTY immediately after processing each incoming output chunk (`terminal.process_output`).

- **Enables** pure, dependency-free unit testing of queries and replies by directly inspecting `screen.replies` after feeding VT byte sequences.
- **Enables** safe, decoupled communication from Layer 1 (terminal/screen) back to Layer 3 (PTY writer assembly).
- **Constrains** reply storage to a maximum cumulative limit of 1024 bytes per ingestion batch to guard against memory exhaustion.
- **Requires** coordinates reported by CPR to be relative to the scroll region only if Origin Mode (DECOM) is active, with column reporting relative to the left margin only when Left/Right Margin mode (DECSLRM) is also active.
