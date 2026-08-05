# Synchronous PTY I/O with std::io Traits

**Status:** Completed
**Date:** 2025-07-28

## Context

`harbor-terminal` needs to read from and write to the PTY. The current architecture runs PTY I/O in a separate worker thread (`terminal_worker`) communicating via channels. The user wants Terminal to directly accept I/O handles without introducing async runtimes. Alternatives:

- **tokio AsyncRead/AsyncWrite** — introduces a heavy dependency, forces async into the terminal
- **bytes crate Buf/BufMut** — avoids u8 copies at the cost of an additional dependency with its own ecosystem
- **Standard library Read/Write** — zero additional dependencies, well-understood, thread-compatible

## Decision

Terminal accepts `impl std::io::Read + Send + 'static` for PTY input and `impl std::io::Write + Send + 'static` for PTY output. It spawns an internal thread to block on `read()` and feed bytes into the parser. The `bytes` crate is NOT introduced — `Vec<u8>` buffers suffice for the PTY read path.

## Consequences

- Zero new dependencies for PTY I/O.
- Terminal owns its own I/O thread lifecycle.
- The old `harbor-pty` crate and `terminal_worker` module may be simplified or removed, since the terminal now directly drives PTY reads.
- Write operations (keystrokes, paste) are synchronous — callers block briefly, which is acceptable for terminal input latency.
