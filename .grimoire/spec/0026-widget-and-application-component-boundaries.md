# Widget and Application Component Boundaries

**Spec ID:** 0026
**Status:** Targeted ownership changes exist; structural evidence is separate from native/HMR acceptance.
**Sources:** [ADR 0064](../adr/0064-widget-and-application-component-ownership.md), [0025](./0025-portal-and-configuration-notifications.md), [state/actions](../adr/0029-widget-store-state-action-boundary.md), [native host/HMR](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md), [icons](../adr/0057-widget-vector-icons-without-font-glyphs.md), [terminal/application appearance](../adr/0058-terminal-themes-separate-from-application-appearance.md).

## Requirements
### R1 — Retain Reusable Widget Mechanisms
Ownership depends on a meaningful non-terminal contract without Harbor business types, not consumer count/artwork names or generic callback/action parameters.
Retain in harbor-widget: Align/Padding/SizedBox/ConstrainedBox/Stack; Flex/Row/Column/Flexible/Expanded/Spacer; DecoratedBox/Separator/TextLabel/Icon/Lucide artwork; InteractiveRegion/MouseRegion/Focus/FocusScope; Button/IconButton; ScrollArea/ScrollController/LayoutObserver; Actions<A>/Shortcuts<A>/ThemeProvider; CustomPaint/Portal; Signal/Store<S,A>/Dispatcher<A>, generic theme/style tokens/defaults.
No widget-owned tab/terminal models, config/watch policy, command meanings/defaults, reducers or application effects. Production dependencies point application → widgets, never widget → app/config/terminal. Generic host infrastructure remains R6.

### R2 — Application-Owned Confirmation Preview
Move the current PreviewPane implementation/protocol to application confirmation composition, shared by paste and clipboard-write confirmation (no copies). Preserve app-prewrapped lines, visible-line count/line-height, shared scroll offset and wheel/confirmation-key coordination, displayed text, bounded scrolling and outcomes. Preview decides neither paste permission nor clipboard authorization; no state-container/scrolling redesign. This is not a ban on future reusable viewers or a request to build one.

### R3 — Reusable Notification Presentation, Application-Owned Composition
Retain generic card/content, close/scroll interaction, local pressed/scroll state and Portal in widgets. Caller chooses placement/layout/visual parameters without widget internals; Harbor's top-right is not the only reusable arrangement. App owns content meaning, eligibility, visibility, duplicate suppression, recovery/dismissal history and root composition. No prescribed new type/builder/product mode.
Preserve all 0025 contracts: top-right resize/DPI placement, non-modal/manual-dismiss/persistent errors, no focus theft/empty-overlay blocking/terminal allocation/grid/PTY resize, selective notices and strict acceptance. No changed appearance/success feed or dismissal resets on routine tab updates.

### R4 — Application-Level UI and State Are Not Tab-Owned
Main workspace/root, notification state and keybinding publication have application-level ownership; tabs own snapshots/display/tab interactions. Renaming a tab controller without separating responsibilities is insufficient. Confirmation/shared preview is not tab-owned.
Workspace/tabs/notifications/confirmation/product appearance are responsibility groups, not mandated files/types/stores. Preserve Host-published state, one-shot actions and effect ordering, not widget reducers or a new bus. Tab sync/insert/remove/responsive updates cannot lose global state, duplicate writers/actions, resurrect dismissed failures or leave stale failure/focus.

### R5 — Keep Product Components and Appearance in the Application
App owns tab rail/row/new/close/abbreviations/unread/active/responsive choices; terminal panel/TerminalWidgetBridge/decorations/window inset; confirmation roots/labels/permissions/outcomes; config-notice classification/messages/history; product appearance/presets. Generic tokens/defaults remain widgets; terminal themes remain separate. Bridge ownership does not move terminal engine/PTY/renderer domain into presentation.

### R6 — Preserve Native Hosting and the Thin HMR Entry
harbor-app-ui remains thin reloadable-root delegation, not a destination for all app UI. Normal/HMR roots use app composition. Feature-gated widget winit adapter owns generic per-window creation/Runtime/GPU/presentation/input/scheduling and optional root reload; application owns event loop, multi-window business routing, platform policy and fatal/exit decisions. Drop old roots/callbacks before unloading HMR generation; retain Host business state.

### R7 — Behavior and Migration Contract
Preserve tab identity/selection/close-without-activation, nested close event handling, focus, responsive/long-label/unread/new-terminal behavior, consumed/pass-through shortcuts, exactly-once command/effect order, terminal/session identity/allocation and confirmation security. Paste/clipboard-write remain separate native flows, not notifications. Icons remain independent of terminal metrics; appearance separation remains.
Exports/callers/tests must agree with chosen layout. Public-path compatibility is not silently decided: clarify any unapproved external break; neither break nor shim is mandated. No new crate/universal controller/notification bus/general viewer/appearance mode/behavior redesign.

## End-to-End Acceptance
| ID | Scenario → required outcome |
| --- | --- |
| E1 | Non-terminal root uses retained layout/button/icon/scroll/focus/generic actions → no Harbor models/reverse dependencies; R1 capabilities remain generic. |
| E2 | Paste and clipboard-write native confirmations with long preview, wheel/keys/accept/cancel → shared app preview, unchanged bounds/text/permissions/window behavior. |
| E3 | Widget notification harness with caller-chosen visual/layout parameters, plus Harbor notice → generic presentation; app retains Portal-backed behavior without new product mode. |
| E4 | Publish notice/bindings, switch/add/close tabs, cross rail breakpoint, dismiss/repeat/recover → independent app ownership, no lost/duplicate/resurrected/stale state, independent notice sources retained. |
| E5 | Focused terminal plus notice show/update/dismiss, resize/DPI/outside/close clicks → paint/hit agreement, no blank-overlay block/focus theft/expiry/allocation/PTY resize. |
| E6 | Tab activate/nested close/focus/compact-expanded/long title/unread/new terminal and shortcut flows → unchanged identity/layout/text/icon/routing; consumed once, pass-through preserved. |
| E7 | Normal and supported HMR roots, Host sessions/UI retained → same app composition, old root/callback teardown before unload, thin entry and host boundary intact. |
| E8 | Structural/state/dependency audit plus affected consumers/native-host/HMR builds → correct preview/notification/global ownership, no duplicate owner/reverse dependency, coherent exports/callers/tests and explicit R7 compatibility. |

## Verification
Inspect [widget inventory](../../crates/harbor-widget/src/widgets/mod.rs)/[dependencies](../../crates/harbor-widget/Cargo.toml), [shared preview](../../crates/harbor-app/src/confirmation/preview.rs), [workspace](../../crates/harbor-app/src/workspace.rs), [notification](../../crates/harbor-widget/src/widgets/notification.rs), [thin entry](../../crates/harbor-app-ui/src/lib.rs) and existing Portal/interaction/tab/confirmation/configuration coverage. Relocate/adapt tests without losing assertions. A moved file alone proves no behavior; screenshots alone prove no ownership. Apply [validation policy](../../docs/validation.md); record actual native window/focus/DPI/HMR checks versus NOT RUN, not a source-based runtime pass. Naming/module/API strategy remains implementation choice subject to R7.
