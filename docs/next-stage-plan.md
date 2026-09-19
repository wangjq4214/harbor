# Next-Stage Product Plan

## Goal and Scope

Make Harbor a reliable Windows daily-use terminal, then add a productive pane workspace and a recognizable visual identity. Correctness, input fidelity, readable text, and bounded resource use take precedence over effects or protocol breadth.

This is the accepted **planning scope**, not an implementation-completion report. Work is planned, investigation-gated, or deferred as indicated below; inclusion is not a completion claim. Existing foundations are recorded in [Current Status](current-status.md); delivery order is owned by the [Roadmap](roadmap.md). Acceptance follows [Validation](validation.md).

Windows remains the active product target. WSL and SSH are compatibility workloads inside the Windows application, not delivery of a native Unix PTY. Native Linux/macOS runtime work remains a later milestone.

## Work Packages at a Glance

| ID  | Work package                                  | Starting point                                          | Main dependency                                     |
| --- | --------------------------------------------- | ------------------------------------------------------- | --------------------------------------------------- |
| N01 | Resize reflow and stable content anchors      | Planned; resize is currently non-reflow                 | Logical-line, width, and coordinate contracts       |
| N02 | Unicode text correctness                      | Planned; wide cells exist, combining text is incomplete | Coordinate contract shared with N01                 |
| N03 | `chcp` and legacy Windows compatibility       | Investigate; no specific Harbor defect assumed          | Reproducible ConPTY probes                          |
| N04 | Configuration hot reload                      | Planned; startup TOML exists                            | Live-update boundary; N01 for font-driven resize    |
| N05 | Command palette                               | Planned; command registry exists                        | Existing command dispatch and availability rules    |
| N06 | Named session profiles                        | Planned; one shell configuration exists                 | Session creation and configuration validation       |
| N07 | Pane workspace                                | Planned; one terminal per tab exists                    | N01; independent session/layout identities          |
| N08 | Scrollback search                             | Planned                                                 | N01 anchors and N02 text representation             |
| N09 | Shell-integration workflows                   | Planned; OSC 7/133 metadata exists                      | Stable anchors; N06 for launch-directory policy     |
| N10 | Kitty keyboard protocol                       | Investigate transport, then implement                   | ConPTY feasibility; richer input event contract     |
| N11 | Compatibility-driven terminal extensions      | Planned bounded slices                                  | Focused protocol tests and application evidence     |
| N12 | Kitty graphics protocol                       | Investigate transport, then implement                   | Resource limits, rendering, N01/N07 placement rules |
| N13 | Cohesive theme and glass-style UI             | Planned polish; Acrylic and decorations exist           | Theme tokens; N04 for live customization            |
| N14 | Liquid-glass refraction prototype             | Investigate, then decide production scope               | Defined sample source, N13, GPU measurements        |
| N15 | History capacity and performance budgets      | Planned configuration/measurement work                  | N01 eviction semantics; measured hotspots           |
| N16 | Diagnostics, acceptance, and Windows release  | Ongoing obligation; release evidence incomplete         | Every delivered slice                               |
| N17 | Extended workspace and cross-platform backlog | Deferred, retained in scope                             | Stable Windows workspace and release gates          |

## A. Reliable Content and Windows Compatibility

### N01 — Resize Reflow

**First delivery**

- Re-wrap retained main-screen and scrollback logical lines when column count changes; preserve explicit newlines rather than concatenating every physical row.
- Define meaningful trailing blanks, styled blank cells, wide-character boundary padding, hyperlinks, and cell attributes. Do not use unconditional text trimming as the reflow algorithm.
- Map the live cursor, saved cursor where applicable, viewport review position, and selection endpoints to the new geometry.
- Give alternate-screen content a separate resize policy appropriate to full-screen applications. Resizing while the alternate screen is active must also resize the saved primary screen correctly.
- Keep PTY and model geometry consistent on failure. Handle width-only, height-only, simultaneous, and repeated changes.
- Define history-capacity eviction during reflow, including invalidation of evicted selections and metadata anchors.

**Design boundary**

Current selections use physical-row generation coordinates. A width change alters physical-row count, so retaining the old generation/column pair is not a content-preserving mapping. Decide on logical anchors or an explicit remapping result before implementing search and command navigation. This does not require replacing the entire ring buffer with a new storage architecture.

The current non-reflow decision in [ADR-0018](../.grimoire/adr/0018-non-reflow-resize-preserved-soft-wrap-markers.md) remains the implemented behavior until a follow-up decision and implementation replace it.

**Acceptance**

Round-trip resize preserves retained logical text, attributes, and copy results except for documented capacity eviction. Cover long lines, CJK, explicit blank lines, colored trailing cells, pending wrap, scrollback review, selection, and primary/alternate-screen transitions. Record Windows shell and `nvim` resize sessions plus large-history resize cost.

### N02 — Unicode Text Correctness

1. Preserve combining characters in terminal content and copied text; render them with their base without introducing an extra cell advance.
2. Extend the content/width model for variation selectors and ZWJ emoji sequences, with an explicit policy for unsupported presentation and ambiguous widths.
3. Keep rendering, editing, selection, search, copy, reflow, and damage tracking consistent with the same text representation.
4. Test sequences split across PTY reads, wide-cell edges, overwrites, erases, fallback fonts, and DPI/font changes.

Agree the text and coordinate contracts with N01 first. Deliver in bounded increments; full complex-script shaping and optional font ligatures are not prerequisites for basic combining-mark correctness. IME preedit support is already present and is not evidence that ordinary terminal content handles combining text correctly.

### N03 — `chcp` and Legacy Windows Compatibility

The [ConPTY channel is always UTF-8](https://learn.microsoft.com/en-us/windows/console/pseudoconsoles); client code-page conversion occurs inside the pseudoconsole. Do **not** switch Harbor's VT parser to GBK after observing `chcp 936` or scrape command output to guess an encoding.

Build a small diagnostic fixture and record:

- `cmd` and PowerShell sessions using code pages 936 and 65001, with 437 as an additional legacy case;
- non-ASCII input/output and file names;
- `ReadConsoleA/W`, `WriteConsoleA/W`, and byte-oriented console I/O;
- programs started before versus after a code-page change, where relevant;
- interactive console behavior versus redirected files/pipes and application-owned encoding choices.

**Acceptance:** publish reproducible byte-level observations with Windows/ConPTY version and application mode. Fix a demonstrated Harbor-side defect if found; otherwise document the boundary and application limitations. Passing the matrix, not adding a new decoder, is the deliverable.

## B. Configuration and Discoverability

### N04 — Configuration Hot Reload

Implement an explicit reload command first, then add file watching with debounce and support for editor atomic-save patterns. User configuration reload is separate from debug-only widget-library HMR.

| Setting                                         | Intended application policy                                                          |
| ----------------------------------------------- | ------------------------------------------------------------------------------------ |
| Keybindings, colors, cursor/selection styling   | Apply to live state after validation                                                 |
| Font family and size                            | Prepare resources, update metrics/atlas state, resize affected sessions, then redraw |
| Shell, arguments, environment, launch directory | Affect newly created sessions; do not restart running processes                      |
| Unsupported live setting                        | Report that a restart is required rather than silently ignoring it                   |

- Parse and validate a candidate configuration before committing it on the application thread.
- On reload failure, retain the last valid live configuration and show actionable diagnostics. Preserve the existing documented startup fallback rules separately.
- Update both existing applicable instances and factories for future instances.
- Define theme-baseline versus active OSC color-override precedence, including what an OSC reset returns to after reload.
- Avoid partial font/resource changes if preparation fails. Verify active, inactive, and alternate-screen sessions.

**Acceptance:** valid edits apply without terminating PTYs; malformed TOML, invalid bindings, unavailable fonts, file replacement, and bursts of saves never leave mixed configuration generations.

### N05 — Command Palette

Reuse the existing application command registry and dispatcher. Add searchable labels, shortcuts, availability, keyboard navigation, and execution feedback. The palette is a new presentation of existing commands, not a second command bus.

Include reload configuration, session/profile creation, tab/pane navigation, search, and appearance commands as those features land. Consumed palette keys must not leak into the PTY. Restore terminal focus and IME state correctly when the palette closes.

### N06 — Named Session Profiles

Add named profiles for shell executable, argument array, starting directory, and environment overrides. Expose them through new-session actions and the command palette.

Keep process quoting and validation centralized. Define precedence between profile directory, explicit launch directory, and inherited OSC 7 directory. A profile change affects future sessions only. Do not put credentials or automatic secret persistence into this feature.

## C. Productive Workspace

### N07 — Pane Workspace

Separate ownership concepts before adding split widgets:

```text
Window
  Tab / workspace
    Split tree
      Pane: allocation, visibility, focus
        Session: terminal state and PTY lifetime
```

The current `TabId` also identifies session output. Introduce stable session and pane identities, or an equally explicit ownership model, so late output events cannot target a newly reused pane. Bind external draw IDs deliberately.

**First delivery**

- Horizontal and vertical splits, draggable ratios, focus by click and shortcut.
- Close-and-collapse layout and maximize/restore of one pane.
- Independent pane allocation, DPI conversion, minimum size, and PTY resize.
- Scheduling for every visible pane; continued bounded output processing for hidden sessions.
- Keyboard, copy/paste, mouse reporting, pointer capture, and IME routed to the correct pane; preserve the paste-confirmation safety gate.
- Closing releases the intended session; hiding or switching tabs does not terminate it. No orphaned PTYs or ownership cycles.

**Acceptance:** two and several visible panes resize independently, simultaneous output stays responsive, focus never sends bytes to another session, and close/exit/late-event races do not affect survivors.

Cross-window dragging and persistence belong to N17, not the initial split feature.

### N08 — Scrollback Search

Start with literal search in the current session, next/previous match, highlighting, and navigation without disturbing live output. Match across soft wraps but respect explicit line breaks. Keep matches consistent through reflow and invalidate them when history is evicted.

Add case options and regular expressions later with bounded work, cancellation, and clear behavior during ongoing output. Search must not scan or copy all history on every rendered frame.

### N09 — Shell-Integration Workflows

Build on existing OSC 7 working-directory and OSC 133 prompt/command metadata:

- inherit a usable current directory for a new local session or pane;
- jump between prompts/commands;
- select and copy one command's output;
- expose command completion/exit status where metadata is available.

Treat shell-reported metadata as untrusted: remote URI paths are not automatically valid local directories, and metadata must never trigger command execution or arbitrary file access. Keep fallback behavior for shells that emit no integration markers. Reflow and eviction must update or invalidate command anchors.

## D. Extended Input and Terminal Protocols

### N10 — Kitty Keyboard

**Probe before promising support.** Verify both directions through the supported Windows/ConPTY versions: negotiation reaches Harbor, replies reach the application, and encoded keys survive the input path. Exercise native VT applications, WSL, SSH, and tmux separately; distinguish classic Console input from VT input modes.

Then implement progressive enhancement in tested slices:

1. Query/enable/disable and stack/reset semantics; advertise only implemented flags.
2. Key disambiguation and required event information at the platform-independent input boundary.
3. Press/repeat/release and further requested enhancements as supported by that boundary.
4. Layout/AltGr behavior, application-shortcut precedence, IME duplicate suppression, focus-loss recovery, and application-exit cleanup.

**Acceptance:** exact encoder tests plus application-visible input traces. A successful write to the ConPTY pipe alone is not acceptance. Follow the [Kitty keyboard specification](https://sw.kovidgoyal.net/kitty/keyboard-protocol/).

### N11 — Compatibility-Driven Extensions

Use the [protocol checklist](protocol/checklist.md), not a goal of implementing every historical xterm feature.

Prioritize bounded slices for:

- modern SGR underline styles/color, conceal, and overline;
- OSC 4/104 palette set/query/reset, separate from existing OSC default colors;
- OSC 52 clipboard writes with explicit permission, strict base64/decoded limits, and safe background-session policy; deny reads by default;
- mouse gaps required by target applications: legacy encodings, horizontal wheel, and pixel reporting, without calling existing SGR mouse support absent;
- continuous box-drawing joins and decoration correctness under font/DPI changes;
- capability replies and `TERM`/terminfo behavior consistent with actual support, especially over SSH and tmux. Do not claim Kitty identity merely because one extension is supported.

Each slice needs malformed/fragmented input, reset, bounds, exact reply/input bytes where applicable, and a named application workload. Existing replies, OSC metadata, focus, mouse, IME, and synchronized output need continued regression/runtime evidence, not reimplementation.

### N12 — Kitty Graphics

First verify APC transport and application interoperability through the same target ConPTY paths. Treat graphics as a separate protocol/resource/rendering project, not an extra parser dispatch case.

**First delivery:** direct-transfer static images; support the chosen format subset explicitly; query, transmit, display, and delete; define scrolling, cell placement, clipping, alternate-screen cleanup, pane ownership, and resize/reflow behavior.

Bound encoded payload, decoded dimensions/bytes, image count, aggregate session memory, and GPU residency. Reclaim resources on deletion, eviction, reset, and session close. Unsupported requests must fail or be consumed safely rather than leaking protocol bytes into text.

Animation, shared-memory/file transports, complex placement, and Unicode placeholders are later increments. File access must not be enabled implicitly. Validate an actual image-preview workload, such as `yazi` where supported, against the [Kitty graphics specification](https://sw.kovidgoyal.net/kitty/graphics-protocol/).

## E. Visual Identity Without Sacrificing Readability

### N13 — Theme and Glass-Style UI

Use the existing Acrylic backdrop, widget theme infrastructure, rounded clipping, borders, and shadows. Unify application design tokens for spacing, corner radii, type, colors, focus indication, separators, and elevation. Add theme selection and live customization through N04/N05.

Polish the side rail, tabs, pane boundaries, toolbar/palette surfaces, hover states, and restrained transitions. Apply glass mainly to chrome and overlays; terminal glyphs must remain sharp and high-contrast.

Provide opaque/high-contrast fallbacks and reduced-transparency/reduced-motion options. Check inactive windows, DPI changes, system-caption behavior, and paste-confirmation readability. Lightweight polish can ship before panes; it does not need to wait for N14.

### N14 — Liquid-Glass Refraction Prototype

Define what is sampled before choosing an implementation: Harbor's own scene or the desktop behind the window. System Acrylic does not provide the renderer with a freely sampleable desktop texture.

For application-local effects, evaluate offscreen textures, masks, blur/refraction passes, damage propagation, and shared work across panes. Desktop sampling requires a separate platform/compositor feasibility and privacy design; do not silently add screen capture.

**Exit:** a measured prototype with supported/fallback modes and a production go/no-go decision. Do not require full-scene refraction for the Windows daily-use release. Keep the prototype in scope even if production deployment is deferred for cost or readability.

## F. Capacity, Evidence, and Release

### N15 — History and Performance Budgets

Expose scrollback capacity with a documented row/content and memory budget. Define behavior when the budget changes and when reflow increases physical-row count. Never promise unlimited history.

Measure output throughput, input-to-present latency, resize latency, idle CPU/GPU use, frame/upload work, memory, and atlas residency for one session and multiple visible/hidden sessions. Include large history, colored output, CJK/emoji, font reload, and visual effects.

Use the [optimization plan](performance/optimization-plan.md) for concrete measured work. Re-profile before choosing the next optimization; do not introduce a parser thread or a broad storage rewrite without evidence of the relevant bottleneck.

### N16 — Diagnostics, Acceptance, and Packaging

- Show actionable configuration, shell-launch, and process-exit information.
- Retain useful crash/fatal GPU/PTY logs and a reproducible diagnostic capture path; avoid recording terminal contents or secrets by default.
- Exercise shutdown, child crashes, sustained output, resource exhaustion, and presentation failures.
- Maintain Windows quality gates and parser property/fuzz evidence, distinguishing configured automation from a recorded successful run.
- Package a Windows preview and then a daily-use release with installation/update instructions, configuration migration policy, known exclusions, and representative dogfood records.

Use [Validation](validation.md) for commands, matrices, evidence records, and release gates. Optional protocol completeness and liquid-glass effects are not substitutes for release evidence.

### N17 — Explicit Later Backlog

Retain these as deferred, not forgotten or implemented:

- cross-window pane/tab dragging;
- workspace-layout and profile restoration; distinguish restoring layout from reconnecting to a still-running process;
- persistent/reconnectable sessions only after a separate process-lifetime and security design;
- later search, graphics, keyboard, and text-shaping increments described above;
- native Unix PTY and macOS/Linux runtime/release support after the Windows gate.

Sixel and unrelated extension families are not prerequisites for the first Kitty graphics slice. Revisit them only for a demonstrated compatibility need.

## Start Here

Open three bounded work items:

1. **N01/N02 contract and reflow regressions**, followed by the first reflow implementation.
2. **N03/N10/N12 ConPTY probes**, recording code-page and extension-transport feasibility before committing to large implementations.
3. **N04 manual configuration reload**, initially colors and keybindings, with last-valid-state retention.

Keep one major implementation stream and one short investigation/polish stream active. Expand each package into implementation tickets only when it is ready to start; do not mark the entire roadmap "in progress."
