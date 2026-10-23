# Composable Buttons and Content Measurement

**Spec ID:** 0037
**Status:** Draft
**Date:** 2026-10-23
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) F4/P1 and its verification requirements; refinement conversation accepting the recommended target contract and the three explicit choices (breaking constructor migration, single-line ellipsis, retained InteractiveRegion style facade); [ADR 0083](../adr/0083-composable-buttons-and-shared-control-surfaces.md).

Selected endpoint: one F4 spec, without ticket slicing, implementation planning or code changes. This contract records settled requirements, not implementation or acceptance. F1–F3 contracts remain applicable; their outstanding native/HMR acceptance is not closed by this specification.

## Requirements

### R1 — Button supports convenient and custom content through one slot

Provide these public construction forms:

```rust
Button::text("Save")
Button::icon(LucideIcon::Plus, "New")
Button::new().child(custom_content)
```

- `Button::text(text)` displays text. `Button::icon(icon, text)` displays an icon plus visible text; its text argument is not hidden metadata as it is on IconButton.
- `Button::new()` has no preselected content. Custom `.child(...)` accepts the established `IntoChildView` protocol, including components and existing View values. An unattached button is a valid empty single-slot control, not a cardinality error.
- Button declares the shared `SingleChild` capability, rather than relying on a macro widget-name exception. The conveniences select content in that same replaceable slot; subsequently attaching custom content replaces the convenience content rather than overlaying or appending to it.
- Repeated `.child(...)` replaces the previous choice. A macro child block is one replacement, and an empty block clears the slot, including convenience or state-driven content.
- Static multiple-child violations fail compilation where provable. Dynamic invalid counts use the existing handwritten `ChildConstructionError` / ordinary `view!` construction-site panic policy. Do not truncate or defer validation to layout.
- Support state-driven `.content(...)` using the established InteractiveRegion content-builder contract: `Fn(&mut BuildCx, InteractionState) -> View + 'static`, with UI-local shared ownership. `.content(...)` and `.child(...)` replace one another. A later valid macro block replaces either choice; an empty block clears either.

These rules extend [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md) / [spec 0035](./0035-unified-child-cardinality-and-attachment.md) to Button without changing their construction semantics. The macro and handwritten builder paths must produce equivalent content, geometry, scenes and routed behavior for equivalent declarations.

### R2 — Separate interaction, shared surface and content responsibilities

- Keep the existing shared InteractiveRegion interaction state machine authoritative for hover/pressed/selected/disabled state, activation, focus and capture. Button and IconButton must not copy or independently evolve input handling.
- Shared control-surface logic owns background, border, internal padding, minimum dimensions and focus-ring drawing. Button and IconButton use that shared capability; they must not retain parallel surface implementations merely with similar code.
- TextLabel and Icon own their respective content measurement and painting. Remove the InteractiveRegion label-specific character-count measurement and text-primitive emission path; moving it into another button-specific renderer is not convergence.
- Button composes these capabilities. IconButton remains a supported convenience, not a second interaction/surface implementation.
- The split does not mandate a new public type for every responsibility, a public surface component, a new crate, or an additional Fiber solely to express each responsibility. The state-driven content Fiber required by R5 remains necessary.

The structural part of acceptance requires inspection of the resulting ownership and rendering paths, in addition to behavioral tests. Adding `.child(...)` while leaving region-owned label drawing is insufficient.

### R3 — Preserve theme roles and direct control configuration

- Button resolves `theme.button` by default and exposes `.style(ControlStyle)` as an explicit override. IconButton retains `theme.icon_button` and its existing style override. Sharing mechanics does not make the two theme roles identical.
- Preserve Button/IconButton `.on_click(...)`, `.disabled(bool)`, `.selected(bool)` and `.focus_handle(FocusHandle)` semantics. Ordinary callbacks remain UI-local `Fn(...) + 'static` with shared `Rc` ownership, without `Send + Sync`.
- Built-in text and icon content resolve their foreground from the same control's current state and selected style. Preserve the established disabled/pressed/selected/hovered precedence and focus-visible presentation; normal theme changes must update mounted controls through the existing rebuild path.
- Custom content retains ownership of its colors. Attaching a TextLabel, Icon or arbitrary subtree must not forcibly overwrite its explicit colors or install a new global foreground-inheritance policy. State-driven content receives InteractionState so callers can deliberately choose state-dependent presentation.
- IconButton remains icon-only: preserve `new(icon, label)`, its non-painted `label()` metadata, `icon_size(...)`, `color_with(...)` and their existing override behavior. Do not turn its metadata label into visible text while adding Button's icon-plus-text convenience.
- Preserve `.focus_handle(...)` forwarding to the actual interactive region without an extra logical Tab stop or a wrapper solely for identity forwarding.

Sources: [ADR 0081](../adr/0081-preserve-click-naming-and-standardize-disabled-builders.md), [ADR 0082](../adr/0082-ui-thread-local-widget-callbacks.md), [spec 0036](./0036-ui-thread-callbacks-and-control-api-consistency.md), current `theme.rs` and IconButton API. This is not a mandate to add every IconButton-specific method to general Button.

### R4 — Use TextLabel measurement and explicit narrow-space behavior

- Built-in button text uses TextLabel's renderer-advance geometry path for both content sizing and ellipsis. There must not be a second button/region character-count estimate when renderer resources are available. Resource-free behavior follows TextLabel's existing fallback rather than a new button-only fallback policy.
- Built-in text stays single-line and uses TextLabel's ellipsis behavior when the allotted text width is insufficient. Apply this to both text-only and icon-plus-text buttons, accounting for the icon and internal spacing within the actual available content area. Do not split graphemes through a separate button truncation algorithm.
- Wider subsequent layouts recover the source text through the existing TextLabel contract; truncation must not replace the retained source string.
- Keep built-in button content centered as control content, rather than relying on region-painted label coordinates. Preserve existing surface style dimensions and parent-constraint authority. Icon sizing remains independent of text metrics/fonts under [ADR 0057](../adr/0057-widget-vector-icons-without-font-glyphs.md).
- Custom child/content composition controls its own wrapping, truncation and presentation; the built-in single-line policy is not imposed on arbitrary subtrees.
- Correct text measurement and the selected ellipsis behavior may change old widths and narrow-space appearance. Verify these changes through actual runtime layout and visual samples for long text, Chinese, mixed text and narrow spaces; do not claim old character-estimate geometry is an equivalence target.

No new numeric spacing, padding, size, radius or performance threshold is established by this refinement. Use the existing theme/style and layout capabilities; a new product visual preset is not part of F4.

### R5 — Preserve content ownership, interaction and lifetime

- State-driven Button content has its own child Fiber, hook sequence and subscriptions under [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) / [spec 0034](./0034-deferred-view-root-and-content-slot-boundary.md). It must not borrow the control's hook sequence. Preserve theme inheritance and state through normal stable-identity rebuilds; clear/remove/replacement releases obsolete content subscriptions and resources under existing reconciliation rules.
- Keep deferred construction, explicit ownership, key/type identity and the existing macro guarantees: single expression evaluation, sibling order, children-before-parent expression evaluation and branch laziness.
- Preserve mouse capture/release, move/re-entry, outside release, cancellation, disabling while pressed, and the existing Enter/Space activation timing and repeat rules. No extra wrapper may dispatch a second logical activation.
- Preserve descendant first refusal: activating a nested close control must not activate the parent row/button. Existing nested focus eligibility and traversal semantics remain applicable.
- Surface/content separation must not shrink the interactive region to the text/icon, paint focus rings behind content, introduce extra focus stops, or retain obsolete callbacks. Interaction geometry follows the resolved control box; expected content-measurement size changes do not authorize an unrelated hit-testing redesign.
- Rebuild/replacement, root removal and supported HMR teardown retain their existing callback/resource-release ordering, including old-generation captures released before unloading. No stale content builder or activation callback may remain mounted after removal.

### R6 — Retain InteractiveRegion composition and migrate affected callers

- Retain `InteractiveRegion::style(...)` as a composition entry point backed by the separated shared surface responsibility. Existing `child`/`content`, `on_activate`, `selected`, `disabled`, `focusable`, cursor and focus-handle behavior remain supported.
- Existing application region compositions in tabs, search and palette must not be required to add a new public surface wrapper. Preserve their style effects, explicit padding/material layering, focus/IME ownership, nested close behavior and business intent semantics. Responsibility separation must not accidentally apply surface padding twice.
- Remove the old `Button::new(text)` signature. Migrate its consumers to `Button::text(text)` without a compatibility constructor or alias. Do not rename IconButton's constructor or broaden this into general F6 constructor/export cleanup.
- Migrate affected production callers, tests, examples and documentation together. Include a text button, icon-plus-text button, custom child and state-driven content example; describe the new constructor and slot-replacement rules, theme roles, narrow-space policy, and the distinction from IconButton's non-painted label.
- Keep application commands on Dispatcher and Host-owned reduction/effects. Product glass/material composition, confirmation permission rules and terminal ownership remain application responsibilities under [ADR 0064](../adr/0064-widget-and-application-component-ownership.md); generic button composition does not absorb them.

## Solution and Seams

Use the accepted composition model in ADR 0083. Retain the current interaction state machine; share surface behavior behind the supported public facades; use normal TextLabel/Icon children rather than a region-owned label renderer. Private helper names, storage forwarding and internal arrangement are implementation choices, not a prescribed public component hierarchy.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Description and attachment | Button / `view!` → shared construction protocol | `IntoChildView`, SingleChild capability, replacement/cardinality rules | Equivalent handwritten/macro slot selection without eager component build |
| Interaction and presentation | Shared interaction state → surface and built-in/state-driven content | Current InteractionState and selected ControlStyle, unchanged activation authority | State-consistent surface/foreground without copied input handling |
| Theme and override | ThemeProvider / caller → Button and IconButton | Distinct button/icon-button theme roles; explicit local style | Correct default role, overrides and reactive theme updates |
| Text geometry | Runtime renderer resources → TextLabel → control layout | Measured advances, actual content constraints, existing fallback and ellipsis | Content-driven size and grapheme-safe truncation without a region estimate |
| Content lifecycle | Reconciliation → state-driven slot / custom subtree | Independent slot Fiber, stable identities, inherited theme, standard cleanup | Isolated hooks/subscriptions and correct replace/clear/unmount behavior |
| Event/focus routing | Runtime → existing interactive control → callback | Descendant first refusal, capture, stable optional handle, UI-local callbacks | Existing activation/focus semantics over the resolved control box |
| Application composition | Tabs/search/palette/confirmation → widget facades and Dispatcher | Existing region styling, material order, business intent and Host ownership | Composable buttons without forced new surface wrappers or business-policy migration |

Relevant baseline surfaces are `crates/harbor-widget/src/widgets/{button,icon_button,interactive_region,text_label,icon}.rs`, `theme.rs`, `construction.rs`, existing content-slot/runtime layout tests, `tests/{icon_button,child_construction,child_attachment,component_boundaries}.rs`, application `tab_view_ui.rs`, `workspace.rs`, `palette.rs`, `confirmation.rs`, and `docs/widget/`. This is a source map, not an assertion that every listed file needs modification.

## End-to-End Tests

### E1 — Construct, replace and clear actual button content (R1, R6)
- **Given:** Mounted text, icon-plus-text, empty and custom-content buttons built through both handwritten and macro declarations.
- **When:** Normal runtime layout/painting occurs, then a child setter/block replaces a convenience or state-driven choice and an empty block clears it.
- **Then:** Only the selected content is rendered; text/icon-plus-text differ observably from icon-only IconButton; empty content has no stale text/icon/builder. Equivalent macro/builder declarations agree in structure, geometry, scenes and interaction.
- **Also:** Public compile fixtures accept all selected forms and reject the old `Button::new(text)` and statically invalid multiple children. Dynamic invalid input follows the existing construction error policy.

### E2 — Measure and truncate through real layout (R2, R4)
- **Given:** TextLabel-backed text and icon-plus-text buttons with renderer/font resources, representative unequal character advances, long titles, Chinese and mixed grapheme-containing text.
- **When:** They lay out under unconstrained, narrow and subsequently wider content constraints.
- **Then:** Geometry uses measured content plus the resolved surface dimensions rather than scalar-count widths; built-in text stays single-line, ellipsizes through TextLabel under insufficient width and restores its full source when space returns. Icon space is included in text constraints. Layout remains subject to parent constraints, including extremely narrow widths.
- **Also:** Compare rendered samples and geometry, and exercise the existing no-resource fallback. Inspect that InteractiveRegion no longer measures or paints labels.

### E3 — Theme and custom presentation remain distinct (R2, R3, R6)
- **Given:** Distinct button/icon-button theme colors/dimensions, explicit style overrides, IconButton tint/size overrides, and custom content with explicit colors.
- **When:** Pointer/keyboard state, selected/disabled flags and inherited theme change through normal rebuilds.
- **Then:** Built-in content and surface use the same control state and correct theme role; overrides retain precedence; IconButton metadata is never painted. Custom colors are not overwritten, while an explicit state-driven builder can update its own presentation.
- **Also:** Existing styled tab/search/palette compositions retain their surface/material ordering and do not acquire duplicate padding.

### E4 — Actual routing preserves activation, focus and hit regions (R3, R5, R6)
- **Given:** Enabled/disabled buttons with stable handles, custom content, and a nested close control in an interactive row.
- **When:** Mouse down/move/re-entry/up/cancel, disable-during-capture, Tab/Shift+Tab, handle-based focus restoration and focused Enter/Space paths run.
- **Then:** Existing activation counts/timing, capture release and disabled behavior remain intact; the whole resolved surface remains interactive, keyboard-visible focus is drawn above content, and no extra Tab stop appears. Closing the child does not activate its parent. Application commands retain source/generation qualifications, queue order, permission and Host-owned execution.

### E5 — Stateful content survives and tears down correctly (R1, R5)
- **Given:** A Button content builder with its own hook state, observable subscription/capture lifetime and inherited theme, alongside parent control state and keyed siblings.
- **When:** Interaction rebuilds the content, keyed siblings reorder, a child/content/block replaces or clears the slot, and the root unmounts.
- **Then:** Stable content retains its own state, does not consume the parent's hook sequence, and keeps theme inheritance. Removed content stops receiving updates and releases subscriptions/resources under existing ownership; replacement uses current callbacks only. Focus/key identity and nested routing remain correct.
- **Additionally:** Supported native/HMR acceptance must verify generation teardown ordering with actual lifecycle evidence, not infer DLL safety from a headless test.

## Decisions and Traceability

| Material choice | Source |
| --- | --- |
| Composable single-slot Button, shared interaction/surface, TextLabel/Icon content ownership | F4 recommendation, accepted refinement target, ADR 0083 |
| `new()` / `text(...)` / `icon(icon, text)`; remove `new(text)` without compatibility | Explicit recommendation accepted by the user; ADR 0083 |
| State-driven content and child replace one another; independent slot Fiber | Accepted target, ADRs 0079/0080 |
| Keep Button/IconButton `on_click`, `disabled`, stable handles and UI-local callbacks | ADRs 0081/0082; F3 contract retained by the accepted recommendation |
| Single-line built-in text with TextLabel ellipsis; custom content controls its own policy | Explicit narrow-space recommendation accepted by the user |
| Retain InteractiveRegion style facade; no mandatory new public surface hierarchy | Explicit recommendation accepted by the user; ADR 0083 |
| Preserve distinct theme roles, IconButton metadata/configuration and custom color ownership | Accepted target plus inspected current theme/IconButton surfaces |
| One spec; no tickets or implementation in this refinement | User's approval of the recommended spec-only route |

## Verification and Limitations

- Cover E1–E5 through public API fixtures, mounted widget/runtime interaction and layout tests, content-slot lifecycle tests, and affected application regressions. Test both ordinary custom children and state-driven content; closure compilation alone does not prove state, geometry or routing correctness.
- Run affected widget, macro and application suites, workspace/all-target checks, examples/documentation checks and formatting/diff checks during implementation. Check that production/docs no longer use the removed constructor and that no region-specific label measurement/paint path survives.
- Record real layout and rendered visual evidence for text-only, icon-plus-text and custom buttons across normal, hovered, pressed, selected, disabled and keyboard-focus-visible states. Include long/multilingual/narrow samples; expected measurement changes must be explained rather than hidden by old snapshot expectations.
- Record native/IME/application focus regressions and actual supported HMR generation teardown where available. Unrun checks are explicit acceptance limitations, not passing evidence. F4 does not retroactively complete the outstanding F1/F3 native acceptance.
- No builds, tests, native sessions or runtime acceptance have been executed as part of this specification work. Inspection, document/source comparison and Markdown/diff validation do not establish implementation correctness.
- No unconfirmed semantic assumptions or blocking requirements decisions remain. Private composition details and numeric choices already governed by existing styles/layout are not prescribed here; a needed new behavioral, architectural or product-policy decision must return to refinement before being added to this contract.

## Out of Scope

- F5 general layout extension helpers, F6 general constructor/import cleanup, F7 numeric validation policy, and F8 named-parameter redesigns. Only the explicitly selected F4 Button constructor break is included.
- New universal style/foreground systems, new global event buses, copied interaction state machines, forced public surface wrappers or a Fiber per responsibility.
- Replacing the text renderer/shaping engine, arbitrary SVG loading, terminal-font/icon-font coupling, or redesigning custom content's wrapping/color behavior.
- Macro DSL/reconciler redesign, activation timing changes, callback thread-model changes, business reducers/effects migration or application visual-preset redesign.
- Ticket decomposition, implementation planning, code changes and test execution in this refinement.
