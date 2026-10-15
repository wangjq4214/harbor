# Widget and Application Component Boundaries

**Spec ID:** 0026
**Status:** Draft
**Date:** 2026-10-15
**Sources:** The approved conversation contract below; [ADR 0064](../adr/0064-widget-and-application-component-ownership.md); [project context](../CONTEXT.md).

## Authority and Conversation Contract

- **C1 — Intent:** Reassess which existing components belong in the reusable widget layer and which belong in the application layer, and correct unsuitable boundaries.
- **C2 — Approved disposition:** Retain the existing generic widget mechanisms; move the current PreviewPane into shared application confirmation UI; separate reusable notification presentation from Harbor notification composition and policy; separate application-wide UI/state from tab-specific responsibilities. Do not add a new crate.
- **C3 — Approved constraints:** Preserve observable behavior, existing native-host and domain boundaries, and the thin `harbor-app-ui` HMR entry. Do not create a global notification bus, universal controller, new general text-viewer framework, or new application appearance behavior as part of this change.
- **C4 — Endpoint:** The user explicitly approved the recommendation and a single boundary spec. This refinement records knowledge and writes this spec only; no tickets, implementation plan, production changes or product QA are authorized.
- **C5 — Unselected detail:** Component/type names, final module paths and public-path compatibility strategy were explicitly left unselected. The illustrative workspace/tabs/notifications/confirmation/appearance grouping describes responsibilities, not a fixed directory tree or a requirement for separate stores.

This is an ownership/refactoring contract, not a redesign of product behavior. It supplements [spec 0025](./0025-portal-and-configuration-notifications.md) without superseding its Portal, configuration-classification, focus, lifetime, confidentiality or allocation requirements. Existing state/action ownership in [ADR 0029](../adr/0029-widget-store-state-action-boundary.md), native hosting/HMR in [ADR 0031](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md), reusable icons in [ADR 0057](../adr/0057-widget-vector-icons-without-font-glyphs.md), and terminal/application appearance separation in [ADR 0058](../adr/0058-terminal-themes-separate-from-application-appearance.md) remain applicable.

## Requirements

### R1 — Retain Reusable Widget Mechanisms

A widget belongs in `harbor-widget` when its contract remains meaningful in a non-terminal application without interpreting Harbor business types. Current consumer count and artwork names are not ownership criteria. Generic callbacks and generic action parameters do not themselves make a control application-specific. [C1/C2; ADR 0064 decision 1]

Retain the following capabilities in the widget layer:

| Responsibility | Existing components/mechanisms |
| --- | --- |
| Basic layout | Align, Padding, SizedBox, ConstrainedBox, Stack |
| Flex layout | Flex, Row, Column, Flexible, Expanded, Spacer |
| Display and decoration | DecoratedBox, Separator, TextLabel, Icon and its built-in Lucide artwork |
| Interaction and focus | InteractiveRegion, MouseRegion, Focus, FocusScope |
| Generic controls | Button, IconButton |
| Scrolling and committed geometry observation | ScrollArea, ScrollController, LayoutObserver |
| Action and theme integration | Actions<A>, Shortcuts<A>, ThemeProvider |
| External content and escape | CustomPaint, Portal |
| State and style mechanisms | Signal, Store<S, A>, Dispatcher<A>, generic theme/style tokens and widget defaults |

These mechanisms must not take ownership of Harbor tab/terminal models, configuration acceptance or watcher policy, application command meanings/default bindings, reducers, or application side effects. Keep the production dependency direction from application composition to the widget library, not from `harbor-widget` to `harbor-app`, `harbor-config` or `harbor-terminal`. Generic native-host infrastructure remains governed by R6 rather than being mistaken for application business policy. [C2/C3; ADRs 0029/0031]

### R2 — Application-Owned Confirmation Preview

Move the current PreviewPane implementation and its scenario-specific interface into application confirmation composition. Paste confirmation and clipboard-write confirmation continue to share the preview rather than acquiring separate copies. The widget library must no longer be the owner of this confirmation-preview protocol. [C2; ADR 0064 decision 2]

The current protocol consists of application-prewrapped lines, explicit visible-line count and line height, shared scroll offset, and coordination between widget wheel handling and confirmation-window keyboard handling. Relocation must preserve the existing displayed preview, bounded scrolling, keyboard/wheel behavior and confirmation outcomes; it does not require replacing the state container or changing scrolling semantics. The preview must not decide paste permission or clipboard-write authorization. [C3; observed PreviewPane/confirmation integration]

This is a disposition of the current implementation, not a rule that all text previews are application-specific. A new general text viewer, text layout system or speculative replacement API is not required. [C2/C3]

### R3 — Reusable Notification Presentation, Application-Owned Composition

Retain reusable notification content/card presentation and close/scroll interaction in the widget layer, and retain Portal as the generic root escape capability. The application must be able to choose notification placement, layout and visual parameters without editing widget internals; Harbor's fixed top-right arrangement must not be the only arrangement encoded by the reusable presentation contract. Separate these responsibilities without mandating a new component name, a specific builder API, or additional product notification modes. [C2/C5; ADR 0064 decision 3]

Ownership is:

| Widget/presentation mechanism | Application composition/policy |
| --- | --- |
| Display eligible content and expose dismissal intent | Determine eligibility, message meaning and visibility |
| Card/content drawing and close/scroll interaction | Choose Harbor placement, layout and visual parameters |
| Local pressed and scroll state | Own duplicate suppression, recovery and dismissal history |
| Generic Portal layout/paint/hit-test escape | Compose the notification presentation at the application root |

Keep the existing Harbor presentation and interaction under spec 0025: top-right floating placement after resize/DPI changes, non-modal behavior, manual dismissal, no terminal-focus theft, no blocking through empty overlay space, no automatic disappearance of actionable errors, and no terminal layout/grid/PTY resize merely because a notice becomes visible. Continue the existing selective notification and strict configuration-acceptance policies. This ownership change is not permission to change the current appearance, introduce a success feed or reset dismissed failures during routine tab updates. [C3; spec 0025 R1–R5]

### R4 — Application-Level UI and State Are Not Tab-Owned

Give main-window workspace/root composition, notification presentation state and application keybinding publication application-level ownership rather than storing or exposing them as tab-specific responsibilities. Keep tab snapshots, tab presentation and tab-specific interaction coordination with the tab UI. A workspace/root owner may compose these responsibilities, but merely renaming a tab controller while retaining unrelated ownership does not satisfy the separation. [C2; ADR 0064 decision 4]

Application confirmation UI and its shared preview must not be owned by the tab module. Application responsibility groups are workspace/root, tabs, notifications, confirmation and product appearance; they need not map one-to-one to files, controllers or stores. Preserve the existing Host-published declarative state and one-shot action transport rather than moving reducers/effect execution into widgets or inventing a new bus. [C2/C3/C5; ADR 0029]

Tab synchronization, tab insertion/removal and responsive presentation updates must preserve application-wide notifications, dismissal/recovery state and keybindings. Moving ownership must not introduce duplicate state writers, duplicate action delivery or stale notification/focus state. [C3; existing TabUiController publication/synchronization contract]

### R5 — Keep Product Components and Appearance in the Application

Keep the following application-owned, using reusable widgets rather than moving their business meaning into the library: [C2; ADR 0064 decisions 4/5]

- Tab rail/row composition, new-terminal and close-tab controls, tab abbreviations, unread/active display choices and responsive rail rules.
- Terminal panel composition, TerminalWidgetBridge integration, terminal decoration presets, window inset and product styles.
- Paste/clipboard confirmation roots, labels, permission semantics and outcome handling.
- Configuration-notification classification, message content, duplicate suppression, recovery and dismissal history.
- Application appearance choices and product presets. Generic widget theme tokens/defaults remain reusable, and terminal palettes remain separate under ADR 0058.

Do not move terminal engine, PTY or terminal-renderer ownership into application presentation merely because the terminal embedding bridge belongs there. Existing terminal-domain responsibilities remain unchanged. [C3; ADR 0031]

### R6 — Preserve Native Hosting and the Thin HMR Entry

Keep `harbor-app-ui` as the thin reloadable-root entry which delegates to application composition; do not make it the destination of all application UI implementation solely because of its name. Normal and HMR roots continue to use the application-owned composition contract. [C2/C3; observed HMR entry]

Keep generic per-window creation, Runtime/GPU/presentation/input/scheduling infrastructure and optional root-reload lifecycle in the feature-gated widget winit adapter. Keep event-loop ownership, multi-window business routing, platform window policy and fatal/exit decisions in the application. Preserve dropping old roots/callbacks before unloading an HMR generation and retaining Host-owned business state. [C3; ADR 0031]

### R7 — Behavior and Migration Contract

This reorganization must preserve tab identity/selection/close behavior, nested close-button event handling, focus behavior, responsive layout and label behavior, resolved shortcut consumed/pass-through semantics, action/effect ordering, terminal/session identity and allocation, and existing confirmation security gates. Paste and clipboard-write confirmation remain separate native confirmation flows; they are not converted to notifications. Preserve icon metric independence and the existing distinction between terminal themes and application appearance. [C3; ADRs 0029/0031/0057/0058 and existing application call paths]

Affected in-repository consumers, exports and tests must remain coherent with the selected module/API layout. C5 does not select an external compatibility promise, a breaking public-path removal, or a compatibility shim. Before committing to a public-path break whose acceptability is not established, return that decision for clarification; do not infer permission from the component's new owner. Exact naming and compatibility policy are not prerequisites to this responsibility-level specification. [C3/C5]

## Solution and Seams

Use the existing crate boundaries. Keep reusable UI mechanisms in `harbor-widget`; move the current confirmation preview into application composition; make notification presentation composable independently of Harbor placement/policy; and separate workspace/global UI ownership from tab-only concerns. This describes the selected ownership solution, not implementation steps, a controller design or a migration sequence. [C2/C5]

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Reusable controls | Application composition → widget controls | Content, style/layout choices and callbacks/generic intents, not business model interpretation | Generic UI behavior without app/config/terminal ownership or reverse dependencies |
| Confirmation preview | Native confirmation host/application confirmation root → shared application preview | Prepared lines, current preview metrics/viewport and existing scroll coordination | Same visible preview and bounded keyboard/wheel interaction; permission remains outside the preview |
| Notification content and intent | Application presentation state → reusable notification presentation | Eligible message/content, appearance/composition choices and dismissal callback | Reusable display, local UI state and dismissal intent without configuration classification |
| Notification placement | Application root composition → Portal/layout → notification content | Harbor placement and size/style choices plus declaring lifetime | Top-right Harbor presentation, correct visual/hit geometry and removal without occupying terminal layout space |
| Application UI state | Host publication/action reduction ↔ workspace/global UI and tab UI | App-level notices/keybindings distinct from tab presentation; existing one-shot intents | No lost global state on tab sync, no extra effect owner, and preserved event-turn ordering |
| Terminal allocation | Application panel/geometry observation → Host/terminal bridge | Committed terminal content geometry and unchanged notice-independent workspace constraints | Existing terminal allocation/resize semantics, with no notice-induced grid changes |
| Reloadable root | Normal/HMR root factory → application composition → widget host | Stable application inputs, retained Host state and safe generation teardown | Equivalent root behavior without moving business state into the dynamic entry or reusable Runtime |

### Verified Source Surfaces

The source snapshot for the ownership observations is commit `dd3695f` before these documentation changes. These paths identify current responsibilities, not mandated post-refactoring paths:

- [Widget inventory](../../crates/harbor-widget/src/widgets/mod.rs) and [crate dependencies](../../crates/harbor-widget/Cargo.toml).
- [Current PreviewPane](../../crates/harbor-app/src/confirmation/preview.rs), [confirmation/root composition](../../crates/harbor-app/src/confirmation.rs) and [native confirmation host](../../src/dialog.rs).
- [Current Notification](../../crates/harbor-widget/src/widgets/notification.rs), [Portal](../../crates/harbor-widget/src/widgets/portal.rs) and [Portal/notification regression tests](../../crates/harbor-widget/src/runtime/portal_tests.rs).
- [Current tab/global state ownership and workspace tests](../../crates/harbor-app/src/tab_view.rs), [application command contract](../../crates/harbor-app/src/command.rs), [Host tab coordination](../../src/tab_coordinator.rs) and [configuration reporting](../../src/shell.rs).
- [Terminal integration and appearance preset](../../crates/harbor-app/src/terminal_view.rs), [theme tokens](../../crates/harbor-widget/src/theme.rs) and [thin HMR entry](../../crates/harbor-app-ui/src/lib.rs).

## End-to-End Acceptance

Structural evidence is necessary for this ownership change; unchanged screenshots alone do not prove responsibility separation. Conversely, a moved file alone does not prove behavior preservation.

| ID | Given / When | Required outcome |
| --- | --- | --- |
| E1 — Generic library use | Build a non-terminal widget test root using retained layout, button/icon, scrolling, focus and generic action mechanisms with test content/callbacks | It does not require Harbor app/config/terminal models or reverse production dependencies. R1's retained capabilities are not migrated into application policy merely because their current production consumer is Harbor. |
| E2 — Shared confirmation preview | Open paste confirmation and separately a clipboard-write confirmation with enough prepared content to scroll; use wheel, keyboard navigation and each confirmation/cancellation path | Both use the application-owned shared preview and retain their existing text, viewport/scroll bounds, native-window behavior and separate permission semantics. The generic widget library does not own the confirmation-preview protocol. |
| E3 — Composable notification presentation | In a widget test harness, supply notification content and dismissal intent with caller-chosen layout/visual parameters; separately show the application's configuration notice | Reusable presentation is not locked to Harbor placement/policy or app data. The application still composes its Portal-backed top-right notice with existing visual/interaction behavior; no new product notification mode is required. |
| E4 — Global state survives tab changes | Publish an actionable notice and resolved keybindings; then switch, add/close tabs and cross the existing responsive rail boundary; dismiss/repeat/recover the relevant notice source | App-level notice/keybinding ownership is independent of tab-specific data. Tab operations do not lose settings, duplicate messages/actions, resurrect dismissed repeated failures or leave stale failures after recovery; established independent-source behavior remains intact. |
| E5 — Notice/terminal coexistence | With a focused running terminal, show/update/dismiss a notice, resize/change scale, click outside it and use its close control | Displayed and hit-test geometry agree; empty overlay space does not block input; focus is not stolen; errors do not time out; notice visibility alone does not alter terminal allocation/grid or request a PTY resize. Existing spec 0025 behavior remains satisfied. |
| E6 — Tab and command regression | Exercise tab activation, nested close-button clicks, keyboard focus, compact/expanded rail, long titles, unread indicators and new-terminal reachability; invoke consumed and pass-through shortcuts | Tab identity/selection, close-without-activation behavior, layout/text/icon behavior and command delivery are unchanged. Consumed commands enqueue once; pass-through input continues to the terminal under the existing command contract. |
| E7 — Normal and reloadable roots | Construct the normal root and, where supported, replace an HMR root while the Host retains sessions and application UI state | Both use the reorganized application composition. Host business state survives; old roots and callbacks are dropped before generation unload; `harbor-app-ui` stays a thin entry and widget native-host ownership is unchanged. |
| E8 — Combined ownership/consumer audit | Inspect the resulting module/state ownership and build affected in-repository consumers, including supported native-host/HMR configurations | Current PreviewPane ownership is application confirmation; reusable notification presentation and Portal remain generic; global notices/keybinding publication are not tab-specific responsibilities; no duplicate competing state owners or forbidden dependency direction is introduced. Exports/callers/tests agree with the chosen layout, with public compatibility handled under R7 rather than silently assumed. |

Use the existing Portal, widget interaction, tab workspace, confirmation and configuration-notice coverage as regression sources. Relocate or adapt tests to the new owner without losing their behavior assertions. Interactive native-window, focus, DPI and HMR evidence must distinguish executed checks from unavailable/NOT RUN checks. No product build, tests or GUI QA are performed by writing this spec. [C3/C4]

## Decisions and Limits

- **Targeted rather than wholesale migration:** C2 and ADR 0064. Retain mature generic mechanisms; move the current scenario-specific preview and clarify notification/application ownership.
- **Separate responsibility, not necessarily types or stores:** C5. `NotificationCard`, a particular workspace controller, an exact directory tree and separate per-feature stores are not selected requirements.
- **No behavior redesign:** C3. Do not implement new appearance modes, change terminal palettes, permission gates, notification eligibility, scrolling semantics or HMR ownership under this contract.
- **No invented product assumptions:** The spec relies on the approved disposition and linked contracts. Final API spelling/module paths remain implementation details; external public-path compatibility remains an explicit decision gate if a break is proposed, not an assumed permission or a prescribed shim.
- **Spec-only endpoint:** C4. No new crate, global notification bus, universal controller, general text-viewer framework, ticket decomposition, implementation plan or implementation/QA execution is part of this refinement.
