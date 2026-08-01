# Widget Runtime Host Simplification

**Spec ID:** 0004
**Status:** Draft
**Date:** 2026-08-02

## Requirement

Harbor must move generic window-event adaptation, runtime scheduling, GPU frame execution, and presentation from the binary crate into `harbor-widget` while the App retains resource ownership, multi-window policy, and Harbor-specific business coordination.

## Solution

Introduce a public, feature-gated winit integration boundary in `harbor-widget` around the platform-independent Runtime.

The core Runtime continues to own the Widget tree, reconciliation, event routing, layout, retained scene, UI-specific GPU state, and dirty-flag update pipeline. It does not expose winit types. The winit integration owns per-window event-conversion state such as modifiers and IME composition, translates supported `WindowEvent` values into `UiEvent`, dispatches them through Runtime, and returns platform operations as `RuntimeEffects`.

Each OS window has an independent Runtime. The App remains the long-lived owner of Window, Surface, Device, Queue, terminal engine, and cross-window policy. For a redraw, the App supplies borrowed references through a frame-scoped `WinitFrameTarget`. The winit integration acquires the SurfaceTexture, updates the viewport, builds the frame, encodes Widget and CustomPaint drawing, submits commands, performs the pre-present notification, and presents the frame without retaining Window, Surface, Device, or Queue.

Surface handling belongs to the frame integration: zero-sized windows suspend rendering; lost or outdated surfaces are reconfigured and redrawn; timeouts, occlusion, and validation failures skip the frame; suboptimal frames are presented and then reconfigured; unrecoverable GPU failures return to the App for fatal-error policy.

The Runtime Frame Scheduler converts input invalidation, external invalidation, animation deadlines, and frame completion into `RequestRedraw`, `Wait`, `WaitUntil`, or `Poll` effects. `about_to_wait` in the App applies these effects but does not implement scheduling policy. `TerminalOutputReady` remains a Host-owned event and becomes a generic external runtime invalidation.

All supported keyboard, pointer, wheel, focus, touch, IME, resize, and scale-factor events pass through the winit adapter. Runtime performs generic routing; `harbor-terminal` interprets terminal-specific actions such as scrollback navigation. Paste confirmation remains application policy implemented with Widget components. The App continues to create and destroy the separate confirmation window, enforce the cross-window input gate, and execute confirmed PTY writes.

Migration preserves observable behavior before deleting the existing `src/app/translate.rs`, App-owned frame scheduler, direct encoder/submission/presentation flow, and terminal input interpretation from `src/app.rs`.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Runtime host integration | App ↔ `harbor-widget` winit integration | Borrowed Window, Surface, Device, Queue, viewport state, and per-window event stream | `RuntimeEffects`, frame outcome, recoverable/fatal surface disposition |
| Platform event adaptation | winit integration → core Runtime | Supported `WindowEvent` values and per-window adapter state | Platform-independent `UiEvent` dispatch without winit types in the core |
| Terminal CustomPaint boundary | Runtime ↔ `harbor-terminal` | Registered external draw handler, routed `UiEvent`, and transient GPU context | Terminal drawing in Widget paint order and terminal-owned input semantics |
| External invalidation | Terminal reader → App → Runtime | `TerminalOutputReady` wake on the UI thread | Generic dirty state and redraw/control-flow effects |
| Cross-window coordination | App ↔ main and confirmation Runtimes | Window identity, confirmation lifetime, and paste-safety state | Per-window routing with App-owned keyboard and paste gate |

## End-to-End Tests

### E2E: Main-window input is adapted and routed

- **Given:** The main window Runtime contains a focused terminal CustomPaint node.
- **When:** The user sends keyboard input, IME composition, pointer input, wheel scrolling, or scrollback navigation keys.
- **Then:** The winit integration converts the event once, Runtime routes it through the Widget tree, `harbor-terminal` applies terminal semantics, and the App contains no equivalent event conversion or scrollback interpretation.

### E2E: Terminal output schedules and presents a frame

- **Given:** The terminal PTY reader posts `TerminalOutputReady` while the event loop is waiting.
- **When:** The App forwards an external invalidation and applies the resulting Runtime effects.
- **Then:** The window requests one redraw, the Runtime integration acquires and renders the surface frame, submits it, presents it, and returns to an idle wait when no additional work remains.

### E2E: Confirmation window renders independently

- **Given:** A confirmable paste has created the native confirmation window.
- **When:** Main and confirmation windows receive redraw requests.
- **Then:** Each window routes to its own Runtime and borrowed frame target, both may share Device, Queue, and text resources, and each frame is acquired, submitted, and presented independently.

### E2E: Cross-window paste safety remains intact

- **Given:** The confirmation window is open.
- **When:** The user types into the main window, starts another paste, scrolls terminal history, or receives PTY output.
- **Then:** The App blocks terminal keyboard input and additional paste requests, while terminal output, rendering, copying, and permitted scrollback continue; confirmation and cancellation preserve existing PTY-write behavior.

### E2E: Resize, DPI, and zero-size behavior

- **Given:** Either window has an active Runtime.
- **When:** The window resizes, changes scale factor, becomes zero-sized, or later returns to a drawable size.
- **Then:** Runtime layout, hit testing, terminal dimensions, and rendering use the updated viewport; no frame is acquired while zero-sized; rendering resumes without stale scale or geometry.

### E2E: Surface failures follow runtime policy

- **Given:** A redraw is requested.
- **When:** surface acquisition reports success, suboptimal, lost, outdated, timeout, occlusion, validation failure, or an unrecoverable GPU failure.
- **Then:** The winit integration applies the defined present, reconfigure, retry, or skip behavior, while fatal failures are returned to the App without Runtime taking ownership of the window or GPU resources.

### E2E: Idle application does not spin

- **Given:** No dirty Fiber, PTY wake, animation deadline, surface recovery, or input is pending.
- **When:** the event loop reaches `about_to_wait`.
- **Then:** Runtime returns `Wait`, the App applies it, and no redraw is requested until a new invalidation occurs.

## Decisions

### Use a feature-gated winit integration around a platform-independent core

- **Choice:** winit event and presentation types are public only through the optional integration module; the core Runtime remains platform-independent.
- **Reason:** This realizes ADR 0015 without adding winit to terminal or core Widget APIs and preserves the crate separations established by ADRs 0004 and 0012.
- **ADR reference:** [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md), [0004-widget-dependency-boundary](../adr/0004-widget-dependency-boundary.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Runtime executes the complete frame using borrowed Host resources

- **Choice:** Runtime integration acquires, encodes, submits, notifies, and presents using `WinitFrameTarget`; the App retains long-term ownership and fatal-error policy.
- **Reason:** ADR 0015 explicitly chooses complete runtime-owned frame presentation without transferring Window, Surface, Device, or Queue ownership.
- **ADR reference:** [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Resolve the ADR 0004 frame-orchestration conflict through ADR 0015

- **Choice:** Treat ADR 0015 as the governing decision for frame orchestration and update ADR 0004's status before implementation planning.
- **Reason:** Completed ADR 0004 assigns shared RenderPass orchestration to the binary, while ADR 0015 and this requirement move encoding, submission, and presentation into runtime integration; both cannot govern the same frame boundary.
- **ADR reference:** [0004-widget-dependency-boundary](../adr/0004-widget-dependency-boundary.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Preserve separate-window ownership while narrowing App rendering responsibility

- **Choice:** The App continues to own and coordinate the confirmation window, but its Runtime integration performs that window's rendering and presentation.
- **Reason:** This preserves ADRs 0007 and 0009. ADR 0015 supersedes the older assignment of per-frame submission and presentation to the App without removing App-owned lifecycle or cross-window policy.
- **ADR reference:** [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md), [0009-app-cross-window-input-gate](../adr/0009-app-cross-window-input-gate.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Keep terminal rendering and input semantics behind CustomPaint

- **Choice:** Runtime performs generic event routing and invokes registered external drawing; `harbor-terminal` interprets scrollback and PTY input semantics and receives transient GPU context.
- **Reason:** This follows the completed CustomPaint injection and terminal-consolidation decisions while avoiding terminal types in `harbor-widget`.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Preserve Runtime and terminal internal architecture

- **Choice:** The migration does not change pull-based Signals, the independent Widget GPU pipeline, parser API, or synchronous PTY ownership.
- **Reason:** These responsibilities are orthogonal to the Host/runtime integration boundary and remain governed by their existing ADRs.
- **ADR reference:** [0002-signal-pull-model](../adr/0002-signal-pull-model.md), [0003-widget-independent-pipeline](../adr/0003-widget-independent-pipeline.md), [0010-parser-minimal-public-api](../adr/0010-parser-minimal-public-api.md), [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md)

### Replace superseded event and presentation assignments

- **Choice:** Follow ADR 0015 instead of superseded ADRs 0008 and 0014; ADR 0001 and ADR 0006 remain superseded by their recorded successors.
- **Reason:** The current design retains per-window Runtimes and a thin Host but moves complete frame presentation farther into runtime than the superseded decisions allowed.
- **ADR reference:** [0001-widget-crate-separation](../adr/0001-widget-crate-separation.md), [0006-custom-paint-input-provider](../adr/0006-custom-paint-input-provider.md), [0008-widget-runtime-for-confirmation-window](../adr/0008-widget-runtime-for-confirmation-window.md), [0014-move-event-adaptation-into-widget-runtime](../adr/0014-move-event-adaptation-into-widget-runtime.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

## Test Plan

- **Integration tests:** Move the existing translation cases into `harbor-widget` adapter tests; cover modifier and IME state, all supported input variants, RuntimeEffects coalescing, scheduler transitions, viewport updates, external invalidation, CustomPaint terminal routing, per-window isolation, and deterministic surface-disposition handling with test doubles where wgpu surfaces cannot be induced reliably.
- **Manual tests:** Run an interactive shell; type composed and non-composed text; use scrollback keys and wheel; resize and move both windows across different-DPI displays; open, confirm, cancel, close, and reopen paste confirmation; minimize and restore windows; verify PTY output continues while confirmation is active.
- **Performance thresholds:** An idle cycle produces zero redraw requests and uses `Wait`; a typical 80×24 terminal frame retains the existing sub-2 ms encode target; migration introduces no additional full-scene rebuild when no Fiber or external draw state is dirty.
- **Edge cases:** Modifier changes during IME composition; focus loss; simultaneous window redraws; terminal wake before first window resume; repeated wake coalescing; zero width or height; stale window IDs; surface suboptimal/lost/outdated/timeout/occluded/validation outcomes; fatal GPU failure; confirmation close during pending PTY output.

## Out of Scope

- Transferring ownership of Window, Surface, Device, or Queue into Runtime.
- Making the platform-independent Runtime depend directly on winit.
- Moving terminal, paste, PTY, or cross-window business policy into `harbor-widget`.
- Replacing the native confirmation window with a main-window overlay.
- Changing Signal reconciliation, Widget rendering primitives, terminal parser behavior, PTY I/O strategy, font loading, or GPU backend selection.
- Creating a general multi-window framework beyond routing Harbor's existing main and confirmation windows.
- Maintaining the old App translation and frame-orchestration APIs after behavior parity is verified.
- Adding speculative clipboard, accessibility, or non-winit presentation integrations beyond the RuntimeEffects contracts already required by current behavior.

## Future Evolution

- Introduce a backend-neutral presentation trait only when a second window/event backend is required.
- Generalize per-window Runtime registration when Harbor adds another independently rendered window type.
- Move more application components out of `src/app.rs` when they can use generic Runtime commands without leaking Harbor policy into `harbor-widget`.
- Revisit shared GPU-resource lifetime abstractions if device recreation or multiple adapters becomes a supported requirement.
- Add deterministic headless presentation tests when wgpu provides a stable surface-independent path matching production presentation semantics.
