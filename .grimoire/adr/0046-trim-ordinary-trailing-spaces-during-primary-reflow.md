# Trim ordinary trailing spaces during primary resize reflow

**Status:** Completed
**Date:** 2026-09-29
**Superseded by (Windows ConPTY live-primary tail retention only):** [ADR 0052](./0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), approved 2026-10-09. The historical completion status and history/non-ConPTY ordinary-tail policy remain unchanged.

## Context

Windows PowerShell pads table rows with ordinary spaces to the right margin. Harbor currently treats all printed spaces as retained reflow atoms, so resizing can produce visually empty continuation rows between entries. ADR-0045 proposed hiding those rows while preserving every tail space for selection and copy, but the current physical-cell-only storage, cursor and selection APIs cannot do that without a coordinated off-grid text and cursor redesign. The user chose the smaller Windows Terminal-style behavior instead.

## Decision

When reflowing a primary logical line on resize, discard its maximal suffix of ordinary default-style, unprotected, non-hyperlinked space atoms before packing at the new width. Do not discard interior whitespace, styled, protected or hyperlinked blanks, explicit blank lines, or the portion needed to preserve a live or saved cursor's insertion position. The discarded tail is not retained for future widening, copy or selection; any selection referring to it may be clamped or invalidated rather than claiming the original spaces survived. Ordinary output parsing and alternate-screen rectangular resizing are unchanged. This supersedes [ADR-0045](./0045-trailing-ordinary-spaces-do-not-drive-visible-reflow.md)'s requirement to retain trimmed tail content; the other content-anchor and buffer-specific policies of [ADR-0042](./0042-content-anchors-and-buffer-specific-resize.md) remain in effect.

## Consequences

- A previously printed ordinary tail may disappear from copied text after resize and will not reappear on widening; this is an explicit exception to the previous content-preserving reflow contract.
- Full-width padded table lines do not produce phantom empty continuation rows on resize. Ordinary spaces needed by the live or saved cursor may still occupy rows so that subsequent output stays aligned.
- In ConPTY's `PreserveLiveTop` mode, if the suffix had occupied the first live row of a logical line that began in history, an empty physical boundary row remains after trimming. It protects producer-relative live coordinates; history must not be pulled into that row.
- Reflow must distinguish ordinary default cells from styled, protected and hyperlinked blanks and preserve true hard line boundaries.
- Resize tests and spec 0014 must state the information-loss exception explicitly; no sidecar storage or copy-path rewrite is required by this decision.
