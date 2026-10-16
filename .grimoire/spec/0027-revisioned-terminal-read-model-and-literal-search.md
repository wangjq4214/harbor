# Revisioned Terminal Read Model and Literal Search

**Spec ID:** 0027
**Status:** Draft
**Date:** 2026-10-16
**Sources:** [Issue #175](https://github.com/wangjq4214/harbor/issues/175), the initial literal-search scope of [#159](https://github.com/wangjq4214/harbor/issues/159), and the user's approval in this refinement conversation of a shared read boundary plus a current-session search entry, highlights and previous/next navigation, followed by Spec → slice. Shell-workflow compatibility follows [#160](https://github.com/wangjq4214/harbor/issues/160); it does not include delivery of that package.
**Delivery tickets:** [Revisioned read model and literal search](../ticket/0018-revisioned-terminal-read-model-and-literal-search/README.md).

## Requirements

### R1 — Bounded read-only content

Expose visible rows and explicitly bounded retained history through a read-only terminal read model without exposing mutable `Screen` internals. Document snapshot lifetime, consistency point, requested scope, copied size, continuation/truncation and invalidation rules. A request must not first clone or decode all history merely to return a bounded subset. Reads must not consume renderer damage or parser events.

A retained logical line may be larger than a read budget or may have an evicted prefix. Bounded reads must preserve enough boundary/offset information for a consumer to distinguish a continuation from a real line start/end and to avoid inventing text or line breaks at a page edge. Reading subsequent portions must not silently combine incompatible revisions into one snapshot.

Source: #175; [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md).

### R2 — Identity and revisions

Every asynchronous read/result must identify its terminal session, relevant buffer/content generation and content revision. Identity must distinguish session replacement, reset and independent primary/alternate identity spaces even when logical offsets or numeric line IDs repeat. The host's current tab or a row's ring index is not sufficient identity.

Document what advances or invalidates the content revision and how a consumer validates it. Existing renderer acknowledgement epochs are not a substitute for a content-validity contract: drawing, acknowledgement, viewport projection and content mutation have different lifetimes. Query replacement/cancellation must also prevent an older query's late completion from replacing the current query's results.

Source: #175; existing `TabId`, `TerminalUpdate` and [content-anchor identity](../adr/0042-content-anchors-and-buffer-specific-resize.md).

### R3 — Shared text and coordinate semantics

Expose retained source text using the established logical-line and Unicode text-unit contracts. Soft wraps join one logical line; explicit line boundaries remain separate, including explicit blank lines. Preserve the distinction between meaningful blanks, unwritten capacity and generated wide-edge padding. Include style and hyperlink metadata sufficient to describe returned content without leaking mutable registry ownership or requiring unbounded metadata copies.

Search must traverse soft wraps through this boundary and must not create matches across explicit line breaks. Match positions must map back to complete retained text units and logical anchors, not assume that a UTF-8 byte, Unicode scalar and terminal cell have interchangeable offsets. Synthetic display cues are not source text.

The read model describes retained content, not text already discarded by capacity eviction or approved resize clipping. In particular, it must not promise restoration of ordinary trimmed tails or Windows ConPTY live-primary styled overflow.

Sources: [reflow spec 0014](./0014-content-preserving-resize-reflow.md), [ADR 0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), [ADR 0035](../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md).

### R4 — Explicit lifetime and invalidation

Document and test the following event contract. Keeping an anchor resolvable does not by itself prove that the text still matches a query.

| Event | Required outcome |
| --- | --- |
| Output append, overwrite, erase or structural edit | Changed content cannot keep an actionable match for different text. Reject stale asynchronous results; preserve or safely refresh meaningful results in retained content as output continues. |
| Width/height change or reflow | Remap through agreed logical anchors, or deterministically invalidate affected results and make the current query usable again through fresh bounded work. Never reuse old physical coordinates as current positions. |
| Capacity eviction, including partial-line prefix eviction | Results/targets referencing lost content cannot remain actionable or resolve to reused ring storage. Surviving content is distinguished from the lost prefix. |
| Primary/alternate transitions, including saved-primary resize | Results are tied to the correct buffer identity; late results cannot decorate or navigate unrelated content in the active screen. Existing buffer-specific resize policy remains unchanged. |
| Hard reset or buffer replacement | Prior content generations cannot be accepted against fresh content, even if IDs or text happen to coincide. |
| Session close or session replacement | Outstanding work is cancelled or made ineligible; late completions and previously presented targets cannot act on a different or closed session. |
| Query replacement or cancellation | Obsolete work cannot publish as the latest query; cancellation work and resource retention are bounded. |

A conservative invalidation strategy is permitted by #175, but repeated output must not turn current-session search into an endless full-history restart with no useful results. The implementation must demonstrate useful progress against retained content while output continues. Deterministic reflow invalidation permitted here is not a claim that all of #159's eventual result-preservation scope has shipped.

Sources: #175, #159 and the existing content-anchor/eviction contracts.

### R5 — Renderer-neutral decoration and action boundary

Represent decoration ranges in terminal-domain coordinates with identity/version provenance. The renderer projects currently valid ranges to visible cells using its current geometry. Search highlighting must not alter cells, source text, parser state or SGR attributes. Existing selection, cursor and preedit behavior must remain compatible.

Define host-action target data sufficient for search navigation and future URL/file consumers, without coupling the read model to GPU handles or executing a target as a side effect of reading/matching. Activation must be validated against current identity/revision and retained content, not merely against the state when a highlight was first displayed. Hyperlink metadata must remain safe after registry cleanup or snapshot expiration.

Keep the logical anchor contract usable by future OSC 133 command navigation; do not create a second physical-row-based identity scheme or reimplement OSC 7/133 parsing. Full command-block collection/navigation is not required in this delivery.

Sources: #175, #160, [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md), [ADR 0037](../adr/0037-osc133-shell-integration-metadata.md).

### R6 — First bounded literal-search consumer

Implement literal search in the current session using the shared read model, rather than an independent mutable-screen or full-history string path. Bound read/copy and scan batches, query/result retention and cancellation checks. Document concrete limits and what happens when a limit or an incomplete read is reached; partial coverage must not masquerade as a complete no-match result.

A user must be able to enter a query, see matching highlights, and move to previous/next matches in retained content without corrupting live output. Search updates and navigation use the same validated result provenance. Changing or cancelling the query makes obsolete work ineligible, including completions that race with cancellation.

Sources: #175, the initial scope of #159 and the approved conversation scope.

### R7 — Real application integration

Provide an application path that validates session identity and revision before accepting asynchronous results and again before activating a stored target. Include a minimal current-session query entry and previous/next navigation. Session switches and late events must not apply one session's results to another; global cross-session search is not included.

Application-specific search composition, command meaning and activation policy remain application-owned. Reusable widget mechanisms remain generic. Reuse the established application command/routing boundary where search operations are user-invokable commands, preserving consumed versus pass-through behavior and avoiding PTY byte leakage from consumed search input. A new global event bus, renderer abstraction or crate split is not selected.

Sources: approved conversation scope; [ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [ADR 0041](../adr/0041-structured-keybinding-configuration.md), [ADR 0064](../adr/0064-widget-and-application-component-ownership.md).

### R8 — Work is bounded outside the render loop

Do not scan or copy the complete history on every rendered frame or hover event. Painting and target presentation must consume prepared results rather than restart searching. Record reproducible large-history work/memory measurements that separate requested scanning from steady-state rendering and show cancellation and ongoing-output behavior. No new numeric performance pass threshold is established by this spec.

Sources: #175, #159 and [validation policy](../../docs/validation.md).

### R9 — Verification and honest completion

Focused tests must cover output arriving during search, text overwrite with a still-resolvable anchor, reflow, eviction, alternate-screen transitions, reset, session replacement/close, cancellation, late-query completion and stale-result rejection. Include bounded reads across long soft-wrapped lines and Unicode/cell-boundary projection. Exercise the real application acceptance/activation path, not only a standalone matcher.

Collect Windows runtime evidence for search/navigation during ongoing output and repeated resize. Record revision, dirty paths, environment/application/ConPTY versions, exact steps, expected versus observed behavior, artifacts and exclusions under `docs/verification/`, following the validation policy. Keep raw local recordings local; committed summaries must be self-contained or link retrievable CI artifacts.

Run applicable implementation gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py`; the validation policy's locked variants may also be used. Record PASS / FAIL / NOT RUN / BLOCKED honestly. Creating this spec/ticket set does not implement or close #175, #159 or #160.

## Solution Boundary and Seams

Build on the existing ring storage, logical text decoding, content anchors and concrete renderer boundary. Public API names, scanning algorithm, worker/thread arrangement, cache organization, UI placement and default shortcuts are not fixed here. They must satisfy the outcomes above without changing the established domain or ownership contracts.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Bounded terminal read | Logical engine → search and future metadata consumers | Explicit scope/budget and identity/revision-qualified continuation | Immutable retained text, logical positions, line boundaries, bounded style/link metadata and lifetime/coverage status |
| Search work | Read model → literal matcher/result delivery | Coherent identity/version, query provenance, finite batches and cancellation | Logical match ranges and explicit completion/partial/stale status without screen mutation |
| Result acceptance and activation | Asynchronous completion / UI → application/session | Current query, session, buffer/content revision and resolvable retained ranges | Accept or reject before decoration publication or navigation; no wrong-session or stale action |
| Decoration projection | Validated terminal-domain ranges → concrete renderer | Current geometry and valid range provenance | Visible highlights without mutable parser/cell access or consumption of unrelated damage |
| Shell compatibility | Existing OSC metadata / future command consumer ↔ shared anchors | Established logical identity, offset, affinity and invalidation semantics | Compatible future command positions without delivering full shell workflows now |

## End-to-End Verification

| Scenario | Given / when | Required observation |
| --- | --- | --- |
| Soft wraps versus explicit lines | A literal occurs across soft wraps; the same fragments also occur on distinct explicit lines; search and navigate | The soft-wrapped occurrence is found/highlighted and navigation reaches its content; fragments across a hard break do not become one match. |
| Output races with search | Delay a result, then append and overwrite text before it returns; continue output while searching retained text | Stale text is not highlighted/activated. Useful current-query results remain obtainable without an endless complete-history restart. |
| Query completion order | Complete an older query after its replacement or cancellation | The older completion cannot replace the current query's results or revive cancelled decorations. |
| Resize and review | Search retained history, navigate to a match, repeatedly narrow/widen while output arrives | Valid remapped results or deterministic invalidation/fresh results; no old-coordinate highlight, wrong navigation or corrupted live output. Respect the ConPTY retention exceptions. |
| Eviction and reset | Evict a matched prefix or hard-reset while a result is delayed | Old results/targets are ineligible; retained ranges do not alias new content or revived IDs. |
| Buffer/session routing | Search, enter/exit an alternate screen, resize saved primary, switch/close/replace sessions, then deliver late results | Decorations and activation remain scoped to the correct live session/buffer, with no stale action after close or replacement. |
| Unicode and paged lines | Search retained CJK/combining/emoji text and a logical line spanning several read batches | Source-text matching and cell projection agree; generated padding/cues are not searched as source text; page edges are not hard breaks. |
| Large history | Populate history, search once, render repeatedly without search-relevant changes, cancel/replace work and repeat under output | Measured copy/scan work stays within documented batches; steady-state frames/hover do not copy or scan all history; cancellation is bounded. |

## Decisions and Scope Traceability

- #169/#170 and #176 are closed at source inspection. Preserve their resulting behavior and the applicable contracts; issue closure is not fresh runtime acceptance for this delivery. This document does not change historical ADR statuses.
- The user selected a minimal usable current-session literal-search slice, rather than a boundary-only API delivery. This requires application and renderer integration, not the full parent work packages.
- #175 expressly permits reflow remapping or deterministic invalidation. The chosen delivered behavior must be documented and tested; this spec does not mandate a particular cache/revision algorithm.
- Current source inspection at `283d55a0606e88bbc3194cb34771fa2378d73490` found internal logical anchors, a visible-grid `TerminalSnapshot`, renderer-oriented `TerminalUpdate` and stable application `TabId`. It did not establish this issue's shared revisioned history/search boundary or runtime acceptance. Existing OSC 133 metadata is not itself command-anchor navigation.
- No additional matching options, fixed performance thresholds, numeric budgets, new architecture or implementation evidence are assumed. Concrete bounded API/coverage rules and delivered interaction semantics must be documented with their implementation tests; new material product/domain decisions return to clarification.

## Out of Scope

Regular expressions, advanced case options, cross-session global search, automatic file/URL detection or opening, dynamic plugins, arbitrary `Screen` mutation, storage replacement, configurable unlimited history, full shell command/directory workflows, new OSC parsing, pane-workspace delivery and universal renderer hosting. Existing OSC 8 activation policy and terminal/PTY resize consistency remain unchanged.

Completion of this bounded issue does not by itself close #159 or #160; retain and link their remaining accepted scope explicitly.
