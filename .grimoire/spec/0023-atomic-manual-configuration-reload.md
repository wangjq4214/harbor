# Atomic Manual Configuration Reload

**Spec ID:** 0023 (manual reload; distinct from Windows distribution 0023)
**Status:** Accepted (owner-confirmed)
**Sources:** [#172](https://github.com/wangjq4214/harbor/issues/172), [ADR 0059](../adr/0059-atomic-manual-reload-and-terminal-only-fonts.md), [0060](../adr/0060-font-reload-uses-normal-bounded-resize.md), [0061](../adr/0061-manual-reload-compensation-failure.md).
**Extensions:** [0024](./0024-watched-reload-and-launch-settings.md) adds watching/environment/directory. [0025](./0025-portal-and-configuration-notifications.md) supersedes only routine-success/absent-file notice obligations, including E1/E2 below; strict rejection remains.

## Requirements

### R1 — Manual Command and Input Availability
Register typed `app.reload-configuration`, display metadata and default Ctrl+Shift+R in the existing application registry/FIFO dispatcher/executor, without a second bus or palette dependency. Use structured TOML:
```toml
[keybindings.app.reload-configuration]
bindings = [{ modifiers = ["ctrl", "shift"], key = "r" }]
```
Omission keeps defaults, records replace defaults, and `bindings = []` unbinds; no rescue chord. Preserve command availability, cross-window security gates, alternate-screen routing, conditional copy/interrupt and history pass-through, and IME. A consumed invocation, its repeat and release cannot reach the PTY across acceptance, rebinding or failure.

### R2 — Complete, Strict Candidate Validation
Read the current configuration source (installed/unmarked: `~/.harbor/config.toml`; portable path policy remains applicable). Missing/unreadable file, malformed TOML, any invalid recognized field/color/font/shell/clipboard value, malformed or superseded string-array bindings, unknown command, unsupported key/modifier, repeated modifier/binding or cross-command conflict rejects the whole candidate. Application-owned semantic binding validation includes registry defaults. Validate unchanged/non-live fields too; never substitute startup defaults or accept a valid subset.
A candidate is a complete defaults/override document, not a patch: removing an optional override restores its documented default. Unknown ordinary fields remain ignored with actionable warning policy, not supported/restart-required settings. Preserve established font discovery/glyph fallback: an unavailable valid family warns and uses a usable system primary; failure to acquire usable fonts, valid metrics or GPU resources rejects reload.

### R3 — Coherent Acceptance and Failure
The application thread publishes only after complete validation and resource/model preparation. Existing applicable sessions, UI bindings and future factories must agree on one accepted generation. Rejection retains accepted colors, OSC baseline/overrides, bindings, fonts/metrics/resources and factories; no per-tab or per-setting partial success.
Keep model/PTY/render/input/IME geometry coherent; propagate resource, barrier and resize errors. The [ordered reader barrier](../adr/0043-acknowledged-pty-resize-barrier.md) must resume every reader on every path without byte loss/duplication or lost pending damage. Configuration atomicity does not freeze/roll back normal child execution/output.
**C4 exception:** after a later-session resize fails, compensate earlier successful PTY resizes. If compensation also fails, retain accepted settings/resources, resume every reader, attempt model/render/input reconciliation with the last successfully established PTY geometry under the prior font policy, and identify both failed operations and affected/unrecovered sessions. Never claim old geometry restored, success or a partial accepted generation. Test ordinary compensation, double failure and retry separately.

### R4 — Supported Application Policies
| Setting | Policy |
| --- | --- |
| Default/ANSI/indexed, cursor and selection colors | Live to all applicable existing/future sessions, under R6 |
| Application bindings | Publish complete resolved live table without clearing state |
| Terminal family/size | Live existing/future terminal resources; UI fonts independent |
| Shell program/arguments | New sessions only; no child restart/relaunch |
| Clipboard policy | Not live; changed value explicitly restart-required |
Valid restart-required changes may coexist with valid live acceptance, never malformed-candidate partial acceptance. Report effective versus pending settings accurately.

### R5 — Terminal-Only Font Changes and Normal Bounded Resize
Keep application/session/PTY/child identity and buffers; no reset or recreation. Widget/chrome/dialog family, size and metrics remain independent. New primary metrics drive rendering/grid; changed geometry uses [normal buffer-specific resize](./0014-content-preserving-resize-reflow.md), unchanged grid does not manufacture resize.
Primary/history retains newest content within physical-row capacity, with ordinary oldest eviction/tail trimming and [ConPTY live-primary exceptions](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md). Active/parked alternate screens stay rectangular, with clipping/wide-cell repair/redraw and no scrollback; resize saved primary independently. Preserve cursor/review/selection/hyperlink meaning under existing anchor validity; normal clipping/eviction may invalidate, unrelated resets may not.
Update inactive sessions and future resources too. Invalidate font/sequence caches and retained GPU projections so glyphs, box/underline geometry, cursor, selection and input/IME coordinates use current metrics. Normal eviction/clipping alone cannot reject valid reload. No unlimited history, hidden backup, second coordinates or special lossless resize.

### R6 — Configuration Baseline and OSC Precedence
Each session's latest accepted palette is its reset baseline (startup palette initially); failed reload changes neither baseline nor overrides.
- Unoverridden default foreground/background/cursor and indexed slots use new baseline immediately; selection uses configured color, not an OSC slot.
- Preserve explicit OSC 10/11/12 and OSC 4 indexes 0–255 RGB overrides, even equal-to-old-baseline values. Alpha always follows new baseline, not retained OSC RGBA; ordinary sets preserve active alpha.
- OSC 110/111/112 clear only corresponding default overrides and restore latest baseline RGBA. OSC 104 clears selected indexed overrides, or all for empty payload, without resetting default/selection slots.
- Palette/overrides are shared between a session's buffers, isolated between sessions; new sessions inherit baseline only. Default/indexed cells reflect active palette; explicit truecolor cells are not rewritten.
Preserve bounded OSC syntax/rejection, query RGB/framing/capacity and RIS/DECSTR/SGR behavior. Terminal fonts/colors do not recolor chrome, choose application day/night mode or change compositor tint ([ADR 0058](../adr/0058-terminal-themes-separate-from-application-appearance.md)).

### R7 — Actionable Results and Honest Documentation
Under spec 0025, feedback is non-modal application UI/log diagnostics, never PTY injection. Identify path and available parse/setting/binding/resource/resize detail, retained effective settings and actual recovery limitations. Report actionable ignored/font warnings and restart needs; distinguish live/new-session/restart effects whenever reported, without routine success feed or palette dependency. Keep defaults, bindings, policies, OSC reset behavior and exclusions accurate; source/parser tests alone cannot establish runtime status.

### R8 — Startup Compatibility and Delivery Evidence
Startup remains separate: document/read failure uses complete defaults; scalar fallback is local; invalid palette restores whole palette; invalid bindings restore all default bindings without discarding valid unrelated fields. Ownership remains config structural parsing → app command semantics/UI → binary transaction/factories, with text font/metrics and terminal content/resize/render resources in their existing crates. Validate prepared GPU/cache resources on their owning thread; internal transaction containers are not prescribed.

## End-to-End Tests
Each row is required; notification eligibility is qualified above.
| ID | Scenario and required observation |
| --- | --- |
| E1 | Primary, inactive tab and byte-recording PTY; change colors/bindings: every session updates, old binding stops, defaults/unbinding work, identities persist and consumed chords emit no bytes. |
| E2 | Known accepted non-default font/palette/bindings/OSC/factories; each R2 failure mixed with valid edits: no fallback/subset commits, existing/future applicable state retained, output continues, actionable failures reported except silent absence. |
| E3 | Change family/size and restore while prompt/input persists: matching PTY/grid and terminal metrics, unchanged UI fonts/tint, current cursor/decoration/selection/IME geometry. |
| E4 | Near-capacity history, narrow via font size: succeeds with oldest eviction/newest retention and normal surviving/evicted anchor behavior, without resets/backups. |
| E5 | Active/inactive and persistent alternate app; font/color change, redraw, exit alternate, new tab: accepted resources everywhere, rectangular alternate and resized retained primary, isolated OSC. |
| E6 | Two sessions, equal-to-baseline explicit OSC and default/indexed/truecolor cells; change RGB/alpha, query/reset in both buffers: R6 identity/alpha/reset/isolation; failure changes nothing. |
| E7 | Inject font, metric/GPU, barrier and later-session PTY failures with all fields changed: no partial publication, geometry/pending damage/reader resume preserved, retry succeeds; separately inject compensation failure under C4. |
| E8 | Live + shell + clipboard changes: old children unchanged, new tab uses accepted shell, clipboard still prior effective policy with explicit restart notice and truthful classification. |
| E9 | Hold old reload chord across acceptance/rejection, use new chord, normal input/IME and alternate mode: repeat/release never leaks; gates/pass-through and explicit unbinding remain correct. |
| E10 | Startup invalid scalar/palette/binding/document or absent file with valid unrelated fields: existing scoped startup fallback; equivalent live errors reject wholly. |
| E11 | Valid unavailable family versus no usable resource: system-primary warning/fallback with coherent metrics versus rejection; missing-glyph fallback changes neither primary grid nor UI fonts. |

## Test Plan and Delivery Evidence
Use validation policy: focused complete-validation/registry/factory/palette/retention/cache/failure tests plus actual Windows cmd and PowerShell prompt/pending-input typing/Backspace/Enter, inactive/new tabs, persistent nvim redraw/alternate exit/primary restoration, retained-text selection/copy, IME, backdrop/opaque fallback and consumed-chord bytes on the actual PTY transport. Exercise repeated font/size cycles, unavailable-family fallback and failed-preparation retry; requested settings alone do not prove effective resource generations.
Record exact revision/dirty scope, Windows/ConPTY/app versions, expected/observed outcomes, artifacts/exclusions and PASS/FAIL/NOT RUN/BLOCKED. Keep secrets/unrelated terminal text out; durable summaries must be self-contained or retrievable CI artifacts, subject only to the specific package waiver in spec 0024.
Historical #172 wording excludes fonts, unlike this owner-accepted font-inclusive contract; no remote scope edit is claimed. Historical manual/font evidence and its qualification survive in [spec 0024](./0024-watched-reload-and-launch-settings.md#delivery-and-outstanding-acceptance).

## Out of Scope
Watching/environment/directory belong to 0024, not this original slice. No palette UI, HMR change, app appearance/UI font configuration, font-file/profile schema, live clipboard policy, child restart, unlimited/lossless retention or broader OSC protocol.
