# Store OSC 8 Hyperlinks as Cell IDs in a Bounded Screen Registry

**Status:** Proposed
**Date:** 2026-09-17

## Context

OSC 8 state must survive ordinary text, scrolling, copying, and scrollback while preserving Harbor's compact `Copy` cell model. Embedding owned URI strings in cells would make cells non-Copy and duplicate ownership across the ring buffer; retaining every observed URI in an append-only table would allow unbounded session growth.

## Decision

Store an optional compact hyperlink identifier on the active pen and each written cell. Resolve identifiers through a screen-owned registry containing the URI and optional OSC 8 `id`; clean unreachable entries based on current pen, saved pen, and retained cells so superseded links cannot accumulate without bound. Accept control-free UTF-8 URIs up to 2048 bytes and `id` values up to 250 bytes, rejecting rather than truncating invalid or over-limit opens.

## Consequences

- `Cell` remains `Copy`, and existing ring-buffer, resize, scroll, and rectangular-copy operations naturally carry hyperlink identity.
- Wide-character lead and continuation cells receive the same hyperlink identifier.
- Erased or default-filled cells contain no hyperlink, while ordinary writing copies the active hyperlink from the pen.
- SGR reset and DECSTR do not close the active hyperlink; an empty OSC 8 URI and RIS close it, and RIS also discards the registry.
- Registry cleanup must consider scrollback and saved state before recycling an identifier.
- Hyperlink lookup is screen-owned and must fail safely for stale or absent identifiers.
