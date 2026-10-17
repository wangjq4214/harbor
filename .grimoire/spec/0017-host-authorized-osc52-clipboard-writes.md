# Host-Authorized OSC 52 Clipboard Writes

**Spec ID:** 0017
**Status:** Implemented — implementation present; automated gates passed; Windows runtime acceptance pending
**Date:** 2026-09-30
**Issue:** [#174](https://github.com/wangjq4214/harbor/issues/174)

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

Policy authority: [ADR 0048](../adr/0048-bounded-osc52-specific-clipboard-capacity.md) (capacity), [0049](../adr/0049-osc52-confirmation-window-and-foreground-policy.md) (confirmation/focus, superseding 0047), [0050](../adr/0050-osc52-write-subset-and-bounded-request-delivery.md) (subset/delivery), and existing [paste-window](../adr/0007-retain-separate-paste-confirmation-window.md) / [cross-window safety](../adr/0009-app-cross-window-input-gate.md). Reuse the static typed OSC action path from #173; no public extensible router is introduced.

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

## Out of Scope

- OSC 52 reads or asynchronous clipboard-content replies. Future read support requires its own disclosure/permission and abuse-limit issue.
- Primary selection, cut buffers, multi-target writes, arbitrary binary clipboard formats, and other clipboard protocols such as OSC 5522.
- Silent background writes, persistent grants inferred from confirmation, and unbounded payloads/queues.
- User-configurable capacity, direct terminal clipboard dependencies, a generic clipboard-provider plugin API, or a public extensible OSC router.
- Delivering configuration hot reload (#155), pane workspaces (#158), or unrelated diagnostics packages as prerequisites for this bounded slice. Preserve existing settings/session boundaries and coordinate with those packages if they evolve.
- Changing user-initiated selection copy, paste encoding, or the existing paste-confirmation safety gate.
