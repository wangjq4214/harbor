# Parser Minimum Public API

**Status:** Completed
**Date:** 2025-07-28

## Context

`harbor-parser` is the zero-dependency VT/ANSI byte-stream parser. It currently exposes `Parser`, `Perform` trait, `Params`, `Param`, `CsiAccumulator`, and `Utf8State`. The user wants to minimize the public surface while preserving zero-cost abstraction. Alternatives considered:

- **A) Current Perform trait with reduced exports** — hide internal state types, keep trait + Params
- **B) High-level event enum** — ergonomic but allocates per event, loses DCS streaming semantics
- **C) Closure-based registration** — `Box<dyn FnMut>` per callback introduces virtual dispatch overhead on the hot path (every byte)

## Decision

Publish exactly three items: `VtHandler` trait (renamed from `Perform`), `Params` (opaque parameter container), and `Parser` (byte state machine). The trait methods use `&Params` for CSI/DCS parameters and `&[u8]` for OSC payloads. `CsiAccumulator`, `Utf8State`, and all internal states are fully private. `advance()` uses static dispatch via generics — zero overhead.

## Consequences

- Callers implement one trait with 9 methods; all method signatures use only `harbor-parser` types or Rust primitives.
- No allocations on the parse hot path.
- `Params` exposes `iter()` and `sub_params()` for CSI parameter access without exposing internal representation.
- OSC payloads remain `&[u8]` slices to avoid forcing UTF-8 allocation.
