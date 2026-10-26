# Exclusive, Extensible Application Popups

**Spec ID:** 0033
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-22
**Sources:** The user's request to make command-palette/search popups exclusive and centrally managed without enumerating concrete popup kinds; subsequent approval of replacement, cleanup, focus, exclusions and clearing displaced search. Durable direction: [ADR 0077](../adr/0077-extensible-exclusive-application-popups.md).

**Known wording discrepancy:** R5/E6 retain disabled-reason wording; [ADR 0078](../adr/0078-command-palette-disabled-items-without-reasons.md) prohibits palette reason text/tooltips.

## Requirements

### R1 — One active participating popup

The command palette and current-session search panel participate in one centrally managed application-popup lifecycle. At most one participating popup may be open, focused for popup input, or presented at a time. Exclusivity must hold in state and lifecycle, not merely by hiding one of two independently open panels during painting.

Opening a different participating popup closes the old one and opens the requested one. Do not queue requests for later automatic display, suspend old popups, or automatically restore a displaced popup when its replacement closes. Both existing entry paths must use the same management contract, including opening search by selecting its command in the palette.

### R2 — Extension without concrete-kind enumeration

Provide one extensible lifecycle interface for participating popups, covering their activation, presentation/focus destination and close cleanup. Its exact Rust API, identifier/handle strategy, storage representation and module/type names are implementation choices.

Adding a future popup must not require adding a concrete variant to a central popup-kind enum, modifying a central list/dispatch of concrete panel kinds, or teaching each existing popup which other kinds to close. Popup-specific presentation and business behavior remain with their respective owners. This does not prohibit the existing typed application-command enum or other unrelated enums.

### R3 — Replacement is a real close

Replacement runs the displaced popup's close cleanup, retiring its editor/input lifetime rather than leaving it hidden and actionable. For search, clear the query, results, selected match and search highlights, and cancel/invalidate pending work and delayed busy presentation. Reopening search starts empty with `No Result`; closing the replacement must not restore the old query/results. Clearing search must not change terminal source content or act as a new navigation request.

For the palette, replacement discards its query/selection and invalidates pending selections and editor deliveries under the existing [palette lifecycle contract](./0030-command-palette.md). A stale close, queued selection, delayed search completion or clipboard/IME delivery belonging to a retired popup must not close, modify, refocus or execute through its replacement, including a later instance of the same popup kind. Preserve the existing bounded cancellation and provenance guarantees in [spec 0027](./0027-revisioned-terminal-read-model-and-literal-search.md).

### R4 — Destination-aware focus and input

During replacement, the new popup receives its own focus destination. Cleanup of the old popup must not subsequently steal focus back to the terminal or retired editor. Closing a popup without a replacement follows existing terminal-focus restoration; a command that opens search or invokes independent confirmation keeps that operation's destination and existing input gate. Preserve composition-first Escape behavior and reusable editor semantics from [spec 0030](./0030-command-palette.md), [ADR 0069](../adr/0069-command-palette-source-aware-availability-and-focus.md) and [spec 0028](./0028-reusable-single-line-text-input.md).

Consumed popup opening, editing, selection, cancellation and replacement input must not leak to the PTY. Reuse the existing typed application command path and Host-owned effects; the popup manager must not become a second command executor.

### R5 — Current-state command availability

Continue using current shared availability facts for palette presentation and Host revalidation. Because opening the palette closes and clears search, search-close/next/previous entries cannot keep using the displaced search as an available hidden target. Under the existing availability rule in [command.rs](../../crates/harbor-app/src/command.rs), these entries are disabled with the search-closed reason while no search is open; do not remove them from discovery or preserve old results solely to keep them enabled. Search-open remains subject to the existing active-terminal requirement and opens a fresh focused search panel.

This is the consequence of R1/R3 and the existing availability contract, not a new catalog or matching policy. Existing tests that navigate a retained search through the palette must be adjusted to this approved lifecycle rather than treated as a requirement to keep both open.

### R6 — Preserve excluded surfaces and ownership

Notifications do not occupy or replace the exclusive popup slot. A notification may coexist with the current popup, must not take focus, and retains existing visibility, dismissal and three-second expiry policy from [ADR 0071](../adr/0071-three-second-notifications-including-errors.md).

Independent native paste-confirmation windows are not converted into participating main-window popups. Preserve existing separate-window lifecycle, authorization and cross-window input gating from [ADR 0007](../adr/0007-retain-separate-paste-confirmation-window.md) and spec 0030. Exclusion from this manager is not permission to bypass those gates.

Application popup membership, panel state, query/result meaning and command effects remain application-owned. Reusable widget mechanisms must not depend on Harbor popup kinds, command or session types; preserve [ADR 0064](../adr/0064-widget-and-application-component-ownership.md). No relocation into a new crate or universal business controller is selected. Search and palette geometry, material and terminal allocation retain their existing contracts; managing exclusivity is not a visual redesign.

## Solution and Integration Seams

The central lifecycle owns the single active participating popup and coordinates retiring the previous lifetime, close cleanup, presentation and the resulting focus destination. Search/palette supply their own content and lifecycle behavior through the common interface. Host-owned terminal/search effects remain outside widget reducers and are coordinated through the existing action transport. Exact signatures and internal ordering remain implementation choices provided the observable requirements hold.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Activation and presentation | Existing search/palette entry paths and workspace composition → shared popup lifecycle | Requested popup's content, lifecycle participation and focus destination | One active popup; replacement instead of independent open flags or pairwise exclusion |
| Retirement | Shared lifecycle → displaced panel/editor and Host-owned search effects | Current popup lifetime and its cleanup contract | Query/result/highlight/work cleanup; late events cannot revive or affect another popup |
| Commands and completion | Existing typed FIFO/Host → popup lifecycle and focus routing | Validated source/context, actual command outcome and operation destination | Palette-to-search handoff without terminal refocus or duplicate execution; current availability |
| Rendering and editing | Application composition → generic Portal/editor/focus mechanisms | Only the live participating popup and its editor lifetime | One presented interactive panel, existing clipping/layout/input behavior, no PTY leakage |
| Excluded surfaces | Notification and native confirmation owners ↔ workspace/Host | Existing notice policy and confirmation gates | Coexistence without consuming the popup slot; unchanged no-focus-theft/security rules |

## End-to-End Verification

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Search has a committed query, valid highlights and selected result; open the command palette | Only palette remains open/presented; search query/results/highlights are cleared; palette editor focused; no consumed input reaches PTY |
| E2 | E1 completed; close the palette, then explicitly reopen search | Palette close restores terminal interaction without reviving search; explicit search-open starts empty with `No Result` and search editor focus |
| E3 | Palette open; invoke the available search-open command once | Palette retired, exactly one fresh search panel open, search editor focused after old editor teardown, no terminal newline or duplicate command |
| E4 | Search work/busy hint is pending; replace with palette and deliver old completions/deadlines | No revived search, stale count/highlight or focus change; a subsequent fresh search cannot accept old results |
| E5 | Retire popup A for B; deliver A's delayed close, queued selection, clipboard result or IME commit; also repeat after reopening A | Live replacement/new instance remains intact; retired events cannot edit it, steal focus or execute a stale action; no stale input reaches PTY |
| E6 | Search was open; open palette and inspect/invoke search-close/next/previous, then search-open | Closed-search actions disabled/rejected with the existing reason and no old-match navigation; search-open uses the ordinary validated command path |
| E7 | Exercise search → palette → search through supported entry paths repeatedly, including multiple requests before a new frame | Each committed/presented state has at most one participating popup; final requested destination owns focus; no suspended layer resurfaces on close |
| E8 | A popup is active; show/dismiss/expire a notification | Popup remains active and focused; notice coexists and keeps existing expiry/dismissal behavior; it neither replaces nor is replaced by the popup slot |
| E9 | Invoke multiline paste confirmation from the palette | Existing independent native window and input gate apply; palette retires under existing command semantics; exclusivity management neither absorbs the window nor bypasses its gate |
| E10 | Integrate a test-only third popup through the common interface; replace it with search/palette and vice versa | Same exclusivity/cleanup/focus guarantees without changing concrete-kind dispatch or existing popup exclusion logic; no production third panel is required |

## Decisions and Traceability

- R1/R2: original request and ADR 0077 — future popup types motivate centralized, non-enumerated management.
- R3/R4: approved replacement/cleanup/focus recommendation and explicit approval of clearing displaced search; existing search/editor/palette lifecycle contracts constrain late deliveries and PTY isolation.
- R5: R1/R3 plus inspected `command_availability` and spec 0030's shared availability and invocation revalidation; a hidden retained search would contradict the approved close semantics.
- R6: approved notification/native-confirmation exclusions; existing ownership, notification expiry and confirmation contracts remain in force.

## Verification Guidance

Use state/composition and Host-routing tests to observe lifecycle cleanup, actual queue admission/execution, search invalidation, editor ownership and final focus; checking only that one panel is painted is insufficient. Include a test-only third consumer to demonstrate extensibility, plus old-lifetime deliveries after replacement/reopening. Preserve coverage for search status, terminal allocation, command outcomes and excluded surfaces.

Where native focus, IME, clipboard or separate-window gating is involved, implementation acceptance requires Windows runtime observations in addition to headless tests. Report tested revision/dirty paths, expected versus observed results, and explicit omissions; this spec and existing tests are not fresh runtime evidence. No new latency/performance threshold is selected.

## Out of Scope

- No waiting queue, popup stack, suspended-search restoration, production third popup, new popup commands or changed matching/search algorithms.
- No notification redesign, conversion of confirmation windows, editor replacement, material/geometry redesign or new crate.
