# Trailing ordinary spaces do not drive visible reflow

**Status:** Superseded
**Superseded by:** [ADR-0046: Trim ordinary trailing spaces during primary resize reflow](./0046-trim-ordinary-trailing-spaces-during-primary-reflow.md)
**Date:** 2026-09-29

## Context

After a Windows PowerShell table has printed lines padded with ordinary spaces, resizing Harbor can reflow those spaces into visually empty continuation rows. ADR-0042 and spec 0014 treat every printed ordinary space as meaningful and pack it at full cell width; indiscriminately deleting whitespace would instead break retained text, selection, and copy semantics. Windows Terminal's reflow measures a row's last non-space content but cannot always distinguish printed spaces from unused capacity.

## Decision

For primary-screen resize reflow, ordinary default-style, non-hyperlinked spaces at the end of a logical line must not by themselves require an additional visible continuation row. Retain their logical content and copy/selection meaning rather than globally deleting or ignoring them; preserve explicit hard blank lines, styled or hyperlinked blanks, significant interior spaces, and live/saved cursor meaning. This refines the visible projection rule for meaningful blanks in [ADR-0042](./0042-content-anchors-and-buffer-specific-resize.md); its other content-anchor, buffer-specific resize, and transactional rules continue to apply.

## Consequences

- Visible reflow extent and retained logical content extent need not be identical at the end of a line; selection, copy, and content-anchor projections must remain coherent across repeated resize.
- A terminal line padded with ordinary spaces must not gain a visually empty continuation solely due to those spaces when resized.
- Tests must distinguish ordinary trailing spaces from styled/hyperlinked blanks, interior spaces, genuine blank lines, and cursor positions within trailing space.
- Existing [spec 0014](../spec/0014-content-preserving-resize-reflow.md) requires reconciliation before implementation; this ADR does not itself change runtime behavior.
