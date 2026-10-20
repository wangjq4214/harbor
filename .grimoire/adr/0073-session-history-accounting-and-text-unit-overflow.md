# Session History Accounting and Text-Unit Overflow

**Status:** Proposed
**Date:** 2026-10-20
**Sources:** Issue #166 refinement: after approving dual history limits and spec → slice, the user explicitly approved all four recommended boundaries: per-session primary-history accounting, live budget reload with irreversible oldest-first shrink, fixed oversized-text-unit protection, and current active/hidden-tab acceptance with simultaneously visible panes deferred.
**Refines:** [ADR 0072](./0072-dual-bounded-terminal-history.md), supplying its previously unsettled accounting, reload and exceptional text-unit policy.
**Supersedes in part:** [ADR 0044](./0044-unicode-text-unit-and-presentation-width-policy.md), only its unqualified source-text preservation for exceptionally large text units exceeding the fixed protective limit. Ordinary text-unit retention, presentation width, fallback and content-anchor policies remain unchanged. This exception is approved requirements policy, not implemented behavior.

## Context

Physical-row limits alone do not bound a cell's variable-length Unicode suffix. A history byte cap alone also leaves unbounded live-cell growth possible. A protective input policy must therefore distinguish exceptional oversized units from normal combining/variation-selector/ZWJ content and describe what can be copied after protection triggers.

History, fixed grid storage, native/font resources and GPU residency are different quantities. Budget changes also need explicit existing-session behavior so a user is not promised recoverability after older content is intentionally evicted.

## Decision

### Per-session history scope

Each session independently enforces the physical-row and retained-source-content byte limits selected in ADR 0072. The byte budget counts retained history text in UTF-8, not allocator capacity, total process private bytes or GPU memory. The primary history remains governed by these limits when saved behind an active alternate screen. Live viewport content and alternate-screen content are not primary scrollback, and are not included in the history-content byte count. Their storage and other resource overhead remain part of separately measured process/GPU budgets.

Keep existing primary reflow and alternate rectangular-resize distinctions. Oldest-first physical-row eviction applies when either history limit is exceeded; do not split an ordinary text unit to fit the history budget. Existing truncated-line and anchor invalidation/review-fallback semantics remain applicable.

### Live configuration changes

Apply accepted history-budget reloads to existing and future sessions, including saved primary history. Reducing a budget immediately evicts the oldest excess history upon successful acceptance. Increasing it permits future retention but does not restore content already evicted. These settings participate in the established complete-candidate configuration acceptance contract; the decision does not authorize premature eviction from a rejected candidate or partial configuration acceptance. See [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md) and its qualified native-compensation failure in [ADR 0061](./0061-manual-reload-compensation-failure.md).

### Exceptionally large text units

Use a fixed protective limit for an individual retained text unit before it can grow without bound, including while it is in a live primary or alternate viewport. Preserve the already accepted prefix once the limit is reached. Ignore further combining-sequence extensions of that same unit until the next independent text unit; do not turn ignored extensions into separately retained orphan units or insert synthetic source text. Normal units within the limit retain ADR 0044's original text and width semantics. Rendering, copy, search and reflow consume the same accepted prefix; discarded extensions are not recoverable source text.

The limit and normal-unit controls must be tested across PTY fragmentation, and the next independent unit must restore normal processing. A font's inability to render a sequence is still not a reason to discard text. This is a deliberate oversized-input safety exception, not a general license to trim Unicode or ordinary history content.

### Measurements and acceptance boundary

Determine numeric defaults, maximum accepted capacity settings, the fixed text-unit protective limit and performance thresholds through comparable measurements with documented rationale. Do not invent values or treat historical targets as current evidence. Until calibration supplies the values, downstream implementation/acceptance must identify that prerequisite rather than claim a completed bounded-resource delivery.

The current delivery measures an active visible tab with retained hidden tabs. Simultaneously visible pane scenarios remain explicit follow-up acceptance work because panes are not implemented. They are not marked PASS, and package closure must link the deferred work instead of implying full multi-visible-session acceptance.

## Consequences

- History source bytes are an enforceable content ceiling, while fixed storage, temporary overlap, process private bytes and GPU/atlas cost require their own measured evidence.
- A successful budget shrink is intentionally destructive to older history; a later increase is not an undo operation.
- Configuration acceptance, saved-primary handling and content consumers must agree on the same retained content.
- The narrowly approved oversized-input exception must be discoverable in the Unicode contract and must not conceal unrelated width/presentation discrepancies.
- Artifact work may proceed on the settled semantics. Numeric calibration remains an explicit execution prerequisite, not a hidden assumption or an arbitrary default supplied by a spec author.
