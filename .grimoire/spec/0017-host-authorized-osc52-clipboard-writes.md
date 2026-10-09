# Host-Authorized OSC 52 Clipboard Writes

**Spec ID:** 0017
**Status:** Draft — implementation present; automated gates passed; Windows runtime acceptance pending
**Date:** 2026-09-30
**Issue:** [#174](https://github.com/wangjq4214/harbor/issues/174)
**Selected refinement endpoint:** One spec; no ticket decomposition, implementation plan, or implementation in this refinement.

## Requirement

An application running in a Harbor terminal session can request a bounded OSC 52 system-clipboard text write or clear. Harbor applies explicit foreground/session authorization and the configured allow, deny, or confirm policy before the existing host clipboard effect performs any system clipboard operation. Terminal parsing never accesses the system clipboard, and OSC 52 reads remain denied.

### R1. Exact write subset

- Recognize completed `OSC 52 ; selection ; data` writes terminated by BEL or ST, including arbitrarily fragmented input and a split ST introducer/terminator.
- Accept selection `c` or an empty selection, both targeting the system clipboard. Reject all other selection values, including primary selection, cut buffers, and combined/multi-target values; never silently remap an unsupported selection.
- Decode standard base64 with valid padding or valid omitted padding. Malformed encoding and non-standard alphabets are rejected rather than repaired.
- Decoded contents must be valid UTF-8 without embedded NUL. Preserve text, newlines, and tabs; do not normalize or truncate the application-provided text. Binary/non-UTF-8 contents are unsupported.
- Empty decoded contents request clearing the system clipboard through the same permissions and confirmation behavior as a non-empty write.
- Clipboard query data `?` is denied/ignored: no clipboard read, confirmation-for-read, or clipboard contents returned to the PTY.
- Unknown, malformed, incomplete, cancelled, and over-limit sequences do not change the clipboard. Preserve existing cancellation/reset and subsequent-text recovery behavior; do not display rejected protocol bytes as terminal text.

### R2. Fixed capacity and bounded retention

| Quantity | Delivered limit |
| --- | ---: |
| Decoded UTF-8 text | 4 MiB = 4,194,304 bytes |
| Base64 data field | At most 5,592,408 bytes |
| Capacity configuration | None |

- Encoded size and decoded size are independently checked before unbounded retention or allocation. The encoded maximum can also encode a slightly over-limit decoded candidate; it is not a substitute for the decoded check.
- Protocol fields have separately bounded overhead. Only the OSC 52 write subset receives increased retention; other OSC and string-family limits remain unchanged.
- Buffers grow with actual input rather than preallocating the maximum for every session. A limit violation rejects the entire request, never copies a truncated prefix, and does not interfere with later ordinary text or unrelated supported OSC behavior.
- Pending payload retention is bounded throughout terminal-to-host delivery, not merely after a host drains an already unbounded vector. In allow mode, keep at most the latest unexecuted valid write for each originating session. In confirm mode, retain at most one admitted confirmation request and reject additional requests while it is pending.
- Replacing or rejecting pending clipboard payloads must not reorder or discard unrelated terminal output events. Session closure releases its pending work, and confirmation cancellation releases its retained payload.

### R3. Foreground authorization and configuration

- Provide an OSC 52 write-policy setting with the settled values `allow`, `deny`, and `confirm`; omission defaults to `allow`. There is no capacity setting and no read-enabling setting in this delivery.
- Integrate with existing TOML user settings and preserve their established startup/fallback behavior. This spec does not introduce a separate reload mechanism; document the delivered setting name, valid values, default, and application timing in the existing configuration documentation/example.
- A fresh eligible request originates from the current active live tab while the Harbor main window has focus. Inactive tabs, an application without foreground focus, minimized application windows, and stale/closed sessions cannot write under any policy, including `allow`.
- `allow` applies eligible writes without a per-write prompt. `deny` applies no clipboard effect. `confirm` requires explicit per-request approval under R4.
- Preserve the originating stable session identity across decoding, request delivery, pending work, and confirmation. The host validates source liveness and current eligibility before executing the approved effect; do not replace the identity with a tab index or whichever tab is currently active.
- Pending same-session writes in allow mode use latest-write-wins before execution. A denied or invalid candidate is not an eligible replacement write. Coalescing never grants a background or stale session permission.
- Default foreground allow does not mean authorization based on application/process authenticity: OSC 52 requests arrive through terminal output, not proof of a user-initiated editor copy action.

### R4. Independent per-request confirmation

- Use an independent confirmation window following the existing paste-confirmation interaction style, not a main-window confirmation panel.
- Show the source tab, decoded byte size, and a length-limited preview. Do not render the entire potential 4 MiB payload merely to request permission.
- Offer allow-this-request and deny-this-request. Approval never implicitly becomes a permanent session/application grant.
- Keep at most one admitted confirmation request at a time. Preserve the exact content being reviewed; subsequent requests are rejected, not queued, substituted, or used to open additional confirmation windows.
- Cancel a pending request if its source session closes, the active tab changes, or the user switches to another application. A stale confirmation outcome must not write to any clipboard or another session.
- Focus moving from the main window to this request's confirmation window is a narrowly scoped continuation of the already-admitted foreground interaction. It does not invalidate that request by itself and does not grant eligibility to new background requests.
- Revalidate the source and the applicable foreground/confirmation state before executing the approved write. The confirmation window is the only settled exception to strict main-window focus for this interaction.
- Existing user-initiated paste, bracketed-paste encoding, paste confirmation, and cross-window paste safety retain their current semantics. The OSC 52 decision approves a clipboard write, not a paste into a PTY.

### R5. Effects, failure reporting, and capability honesty

- Terminal/parser layers emit constrained typed requests and perform no `arboard` or other system clipboard I/O. Carry bounded decoded contents and originating session identity at the terminal-to-host request boundary.
- Approved writes use the existing host/widget clipboard effect path, including clearing through an empty-text write. Do not create a parallel terminal-owned system clipboard implementation or generic clipboard-provider plugin API.
- Denial/failure produces content-free diagnostics, not extra popup windows, terminal text, or protocol error replies injected into the PTY. Diagnostics and evidence must not contain clipboard text or base64 payloads, including rejected candidates.
- Host clipboard failures are failures, not reported success. Preserve the actual effect outcome in tests/runtime evidence without adding a terminal protocol acknowledgment.
- Do not advertise unsupported reads or broader selections/clipboard protocols through capability replies, protocol checklists, or documentation. Describe only the delivered and evidenced write subset.

## Solution and Necessary Seams

Reuse the private static built-in OSC typed-action boundary delivered by #173. Parsing validates bounded wire data; terminal-owned code emits typed side-effect requests; application-owned routing binds/preserves stable session identity and owns authorization, pending-request policy, and the confirmation lifetime. Only approved requests reach the existing native clipboard effect.

The integration contract is deliberate; exact helper names and private struct layout are not prescribed by this spec.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Bounded OSC framing | `harbor-parser` → `harbor-terminal` built-in OSC handling | Complete/cancelled/overflowed string behavior, BEL/ST fragmentation, OSC 52-specific bounded retention | Validated write action or safe consume-ignore, without system clipboard access |
| Session-qualified host request | Terminal output/update boundary → `harbor-app` tab routing and application host | Stable originating live-session identity and bounded decoded text | Typed host request with same-session latest-write behavior; no effect before authorization |
| Settings and host policy | Existing `harbor-config`/TOML settings → application host | Allow/deny/confirm setting and established settings behavior | Default foreground allow and explicit deny/confirm decisions; no capacity/read configuration |
| Confirmation interaction | Application host ↔ independent confirmation window | Source identity, exact pending payload, bounded display data, explicit decision and focus/lifecycle observations | One per-request outcome with cancellation and revalidation; no implicit persistent grant |
| Native clipboard effect | Authorized application host → widget/winit clipboard effect → OS | Approved UTF-8 text or empty clearing text | Existing clipboard write execution and content-free failure diagnostics |

### Current repository anchors

These identify integration surfaces, not a claim that OSC 52 is implemented:

- `crates/harbor-parser/src/params.rs`: current 4096-byte generic OSC/string limits.
- `crates/harbor-terminal/src/parser/osc.rs`: private static routing into built-in typed actions; OSC 52 is not currently routed.
- `crates/harbor-terminal/src/parser.rs` and `crates/harbor-terminal/src/types.rs`: parser-side output-event collection and terminal output event types.
- `crates/harbor-app/src/tab_manager.rs`: monotonic, non-reused `TabId` and live-tab output draining.
- `src/tab_coordinator.rs` and `src/shell.rs`: session-qualified application output handling and host lifecycle/command coordination.
- `src/dialog.rs`: existing independent paste-confirmation window and application-owned input gate; reuse its interaction style without changing paste semantics.
- `crates/harbor-widget/src/winit/host.rs`: `WinitWindowHost::write_clipboard` applies a write via runtime effects.
- `crates/harbor-widget/src/effects.rs` and `crates/harbor-widget/src/winit/effects.rs`: `ClipboardEffect::Write(String)` and the native `arboard` write/failure path.
- `README.md` startup configuration and `config.example.toml`: existing user-facing configuration documentation.

## End-to-End Tests

Use synthetic, non-sensitive fixture contents. Inspect actual system clipboard changes only in controlled Windows runtime cases; use a recording/failing effect sink for deterministic policy tests.

| Case | Given | When | Then |
| --- | --- | --- | --- |
| Foreground default allow | Live active tab, focused Harbor main window, omitted write policy | A supported OSC 52 text write arrives through PTY output | Exactly the approved decoded text reaches the existing host effect without a confirmation prompt; no protocol bytes become terminal text |
| Empty-selection and clear | Eligible foreground session | A write uses empty selection, or an empty data field | Empty selection targets the system clipboard; empty contents clear it only after the normal allow/deny/confirm decision |
| Explicit deny | Eligible session configured deny and a known fixture clipboard value | Non-empty or clearing request arrives | No clipboard effect and no clipboard change; content-free diagnostic only |
| Background and stale source | Inactive tab, unfocused/minimized Harbor, or closed source session | A write/clear or late host request is processed, including under allow | No clipboard change, no automatic foreground/background authorization, and no retargeting to the active tab |
| Confirm approval | Eligible foreground session configured confirm | A request opens its independent window and the user approves | Correct source/size/bounded preview is shown; confirmation focus alone does not invalidate it; the exact approved content is written once after revalidation |
| Confirm denial and cancellation | A pending confirmation exists | User denies, closes its source, changes tab, or switches applications | No write; pending contents are released; a delayed/stale approval cannot resurrect the request |
| Confirm flood | One request is being reviewed | Additional requests arrive | Original preview/content is unchanged; no additional dialog or unbounded pending queue; additional requests do not write |
| Allow coalescing | Same eligible session has unexecuted writes | Valid A then valid B arrive before execution, with invalid candidates interspersed | Only latest valid B remains pending and is eligible for execution; unrelated events and other session identities are preserved |
| Capacity boundaries | Eligible foreground session and bounded fixture generation | Exactly 4 MiB, 4 MiB plus one byte, and encoded overflow candidates arrive | Exact decoded maximum works; either limit violation rejects the whole request without changing clipboard or copying a prefix |
| Fragmented framing and reset | A stream includes OSC 52 and later ordinary text | Feed BEL/ST variants at different splits, including inside base64 and between ESC and backslash; also cancel/reset incomplete strings | Complete valid writes behave equivalently; incomplete/cancelled requests never write; later text and other OSC handling recover correctly |
| Read/unsupported input | Known fixture clipboard value | `?` query, unsupported selection, invalid UTF-8/NUL, or malformed base64 arrives | No read/disclosure or write; no false capability claim, extra error window, injected terminal error, or PTY reply |
| Host failure and regressions | Approved request with an injected clipboard backend failure; existing selection copy/paste workflows | Apply failed effect and repeat ordinary copy, bracketed paste, and paste confirmation | Failure is diagnosed without payload leakage or false success; existing user-initiated clipboard/paste safety semantics remain unchanged |

## Decisions and Source Traceability

| Requirement group | Settled authority and rationale |
| --- | --- |
| Host-only effects, bounded writes, read denial, runtime evidence | [Issue #174](https://github.com/wangjq4214/harbor/issues/174); built-in typed-action prerequisite [#173](https://github.com/wangjq4214/harbor/issues/173) |
| Fixed 4 MiB, no capacity configuration, OSC 52-only increased retention | [ADR 0048](../adr/0048-bounded-osc52-specific-clipboard-capacity.md); user selected 4M after discussing larger text-copy capacity |
| Default allow, inactive/background/stale denial, configurable confirm and independent per-request window | [ADR 0049](../adr/0049-osc52-confirmation-window-and-foreground-policy.md); explicitly supersedes [ADR 0047](../adr/0047-foreground-only-host-authorized-osc52-writes.md) to allow confirmation-window focus for the pending request |
| Target/text/base64 subset, empty clearing, latest-write replacement and diagnostic-only denial/failure | [ADR 0050](../adr/0050-osc52-write-subset-and-bounded-request-delivery.md); final user confirmation before spec generation |
| Existing paste confirmation and cross-window paste safety | [ADR 0007](../adr/0007-retain-separate-paste-confirmation-window.md), [ADR 0009](../adr/0009-app-cross-window-input-gate.md); preserve these behaviors, do not reinterpret them as OSC 52 permission |
| Existing terminology and startup settings contract | [Project context](../CONTEXT.md), [terminal protocol context](../CONTEXT-terminal-protocol.md): OSC 52 Host-Authorized Clipboard Write, Write Subset, Request Delivery, Parser Retention Limits, Terminal Tab, RuntimeEffects, TOML User Settings |
| Verification and truthful status | [Validation policy](../../docs/validation.md); required gates, protocol honesty, Windows runtime evidence, and content-free artifacts |

The new ADRs are Proposed because they describe accepted future behavior, not completed implementation. This document only expresses and verifies the settled sources above; a new semantic or architectural decision discovered during execution must return to discussion and knowledge recording rather than be silently invented in the spec.

## Test Plan and Definition of Done

### Automated coverage

- Parser/terminal tests cover one-shot and fragmented BEL/ST input, padded/unpadded base64, UTF-8 splits represented in the decoded contents, empty writes, unsupported selections, malformed padding/alphabet, NUL/non-UTF-8, cancellation/reset, capacity boundaries, overflow discard, and subsequent text. Existing OSC built-ins and non-OSC string-family limits retain their prior behavior.
- Include the decoded-over-limit case whose base64 length still fits the encoded cap, not just an encoded-overflow case.
- Exercise parser retention, decoded allocation, and pending-request high-water bounds under repeated large requests and multiple sessions. An unbounded intermediate clipboard-event collection fails R2 even if the host eventually keeps one request.
- Host policy tests cover every mode against focused active, inactive, unfocused/minimized, and stale sessions. Validate source identity and current permission again at effect execution; include source closure and late callbacks.
- Confirmation tests cover source/size/preview bounds, exact per-request approval, no permanent grant, one pending request, rejection of subsequent writes, tab/closure/external-focus cancellation, and confirmation-window focus continuity.
- Settings tests cover omitted/default and all supported policy values through the established loader behavior, without introducing capacity or read settings.
- Application/effect tests prove approved requests reach the existing clipboard effect exactly as selected, denied candidates produce no effect, backend failures are diagnosed without payload leakage, and unrelated terminal events and existing copy/paste workflows remain correct.
- Capability/reply tests ensure read queries disclose nothing and advertised/documented behavior does not exceed the selected subset.

### Windows runtime acceptance

Record reproducible foreground default/explicit allow, explicit deny, confirm approve/deny, inactive tab, external-application focus, minimize/restore, stale-session, malformed, empty-clear, exact maximum, and oversized cases. Confirm the real OS clipboard remains unchanged for rejected requests and receives exact approved fixture text through the runtime application path.

Include a named application workload that emits OSC 52 and record its exact application and transport versions; distinguish a direct synthetic protocol probe from a remote editor or WSL/SSH/tmux compatibility claim. Do not infer transport/application acceptance from parser tests. Repeat ordinary selection copy and paste-confirmation behavior as regression checks.

Keep a self-contained validation summary in committed documentation and retain raw artifacts locally or in retrievable CI, following the validation policy. Record:

- Revision and dirty-tree scope.
- Windows build, ConPTY/application versions, build profile, and actual transport.
- Exact commands or reproducible steps, expected and observed results.
- PASS / FAIL / NOT RUN / BLOCKED, artifacts, and known exclusions.
- No actual sensitive clipboard contents, base64 payloads, or unrelated terminal content in committed artifacts.

### Gates and documentation

Run applicable implementation gates and record any unrun command with its reason:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

Update `docs/protocol/checklist.md`, the existing configuration documentation/example, and any affected retention-limit documentation to state the exact delivered target/text subset, fixed encoded/decoded limits, default policy, background restrictions, confirmation lifetime/focus exception, latest-write behavior, and denied reads. Do not mark unsupported or unexecuted scope complete.

### Acceptance checklist

- [ ] Supported writes/clears traverse a typed terminal-to-host request with stable source identity and the existing host clipboard effect.
- [ ] Encoded/decoded and pending-retention bounds hold; no generic protocol cap is enlarged unintentionally.
- [ ] Default/allow/deny/confirm obey active/background/stale-session rules, including delayed confirmation and execution races.
- [ ] Independent confirmation shows a bounded preview and approves only its unchanged request; additional requests and invalidating lifecycle transitions cannot cause writes.
- [ ] Unsupported, malformed, incomplete, cancelled/reset, and over-limit candidates do not change the clipboard or corrupt subsequent text parsing.
- [ ] Reads remain denied without disclosure or unsupported capability claims.
- [ ] Allow-mode same-session latest-write handling and unrelated event preservation have deterministic tests.
- [ ] Content-free diagnostics report denial/failure without extra popups, terminal text, or PTY error replies.
- [ ] Windows runtime evidence covers the delivered policy and capacity, and preserves existing copy/paste confirmation semantics.
- [ ] Exact policy/limits are documented; focused tests and applicable gate results are recorded honestly.

## Out of Scope

- OSC 52 reads or asynchronous clipboard-content replies. Future read support requires its own disclosure/permission and abuse-limit issue.
- Primary selection, cut buffers, multi-target writes, arbitrary binary clipboard formats, and other clipboard protocols such as OSC 5522.
- Silent background writes, persistent grants inferred from confirmation, and unbounded payloads/queues.
- User-configurable capacity, direct terminal clipboard dependencies, a generic clipboard-provider plugin API, or a public extensible OSC router.
- Delivering configuration hot reload (#155), pane workspaces (#158), or unrelated diagnostics packages as prerequisites for this bounded slice. Preserve existing settings/session boundaries and coordinate with those packages if they evolve.
- Changing user-initiated selection copy, paste encoding, or the existing paste-confirmation safety gate.
- Ticket slicing, execution ordering, an implementation plan, or implementation as part of this refinement.

## Future Evolution

Read support remains a separately authorized security-sensitive feature. Additional selection targets, capacity configuration, or clipboard protocols require a demonstrated interoperability/capacity need and a separately settled contract rather than being inferred from this write-only spec. No other speculative evolution is included.
