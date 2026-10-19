# Widget and Application Component Ownership

**Status:** Proposed
**Date:** 2026-10-15
**Sources:** The user's request to reassess reusable widget versus application components, followed by explicit approval of the boundary recommendation and spec-only endpoint.
**Related:** [ADR 0029](./0029-widget-store-state-action-boundary.md), [ADR 0031](./0031-widget-winit-adapter-owns-native-host-infrastructure.md), [ADR 0057](./0057-widget-vector-icons-without-font-glyphs.md), [ADR 0058](./0058-terminal-themes-separate-from-application-appearance.md), [ADR 0063](./0063-root-composable-escape-and-quiet-configuration-notices.md).
**Superseded by:** [ADR 0071](./0071-three-second-notifications-including-errors.md), only the persistent-actionable-error constraint in decision 3; component ownership and all other boundaries remain unchanged.

## Context

At approval, `harbor-widget` exposes both generic controls and the current `PreviewPane`, whose prewrapped lines, explicit visible-line count and shared atomic scroll offset coordinate with application confirmation-window keyboard handling. Its interface reflects that scenario's protocol rather than an independently established general text-viewer contract.

`Notification` already leaves configuration eligibility to the application, but combines reusable card/content interaction with a fixed top-right position and product presentation parameters. In the application, `tab_view::ui` contains the main workspace and confirmation roots, while `TabUiState` and `TabUiController` also carry application-wide configuration notices and keybindings. These responsibilities do not all belong to tabs.

The user approved targeted ownership changes rather than wholesale downward migration of generic controls, a new UI crate, or a new notification framework. This decision refines component and application-composition boundaries; it does not replace the existing native-host, state/action, icon, theme or configuration-notification contracts.

## Decision

1. **Reusable mechanisms remain in `harbor-widget`.** Judge reuse by whether the component's contract still makes sense in a non-terminal application without interpreting Harbor business types, not by the number of current consumers or the names of bundled artwork. Layout, decoration, text/icon display, interaction, focus, buttons, scrolling, geometry observation, generic actions/shortcuts, theme provision, external paint integration and Portal remain reusable capabilities. `Signal`, `Store<S, A>`, `Dispatcher<A>` and generic style tokens remain mechanisms, not owners of reducers or product policy.

2. **Move the current PreviewPane implementation to application confirmation composition.** Paste and clipboard-write confirmation share this application-owned preview. This is a disposition of the current scenario-specific interface, not a prohibition on a future reusable text viewer and not a commitment to build one now.

3. **Separate notification presentation from application composition and policy.** Retain reusable notification content/card and close/scroll interaction in the widget layer, together with generic Portal support. The application chooses Harbor's placement, layout and visual parameters and owns eligibility, message meaning, visibility, duplicate suppression, recovery and dismissal history. Local pressed/scroll state remains with the presentation component. Whether this uses separate component types or a composable existing `Notification` API is not selected. Preserve the established top-right, non-modal, manually dismissible presentation, no focus theft and persistent actionable errors from ADR 0063 and spec 0025.

4. **Organize application UI by responsibility, not under tabs.** Application-level workspace/root composition, notification state and keybinding publication must not be owned as tab-specific state. Tab presentation retains tab-specific display and interaction coordination. Confirmation UI and its shared preview belong to application confirmation composition. Terminal embedding and product appearance presets remain application responsibilities. Workspace, tabs, notifications, confirmation and appearance are responsibility groups, not mandated filenames, type names, or separate stores.

5. **Preserve existing infrastructure and domain boundaries.** `harbor-app-ui` remains a thin reloadable-root entry rather than a destination for all UI implementation. Generic native hosting and HMR lifecycle remain in the feature-gated widget winit adapter; application event-loop, multi-window, platform and fatal/exit policy stay with the application. Terminal themes remain distinct from application appearance; generic theme defaults stay in the widget layer. No new crate, global notification bus or universal controller is selected.

## Consequences

- Reusable component contracts no longer need to encode the confirmation-preview protocol or Harbor's fixed notification composition. Application-wide UI concerns have an application-level owner rather than requiring a tab controller.
- This is a behavior-preserving ownership change. It must retain tab interactions and responsive rules, terminal identity and allocation, command/effect ordering, confirmation permissions and separate native confirmation windows, notification semantics, and HMR teardown/business-state retention guarantees.
- API spelling, final module layout and public-path compatibility strategy remain unselected. The decision does not authorize assuming either an external API break or a particular compatibility shim.
- Existing ADRs listed above remain applicable; this decision clarifies ownership without superseding their behavior contracts. The approved refactoring has not been implemented or accepted by runtime verification as part of this refinement.
