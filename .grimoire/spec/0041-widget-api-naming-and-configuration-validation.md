# U4 — Widget Naming, Named Configuration, and Consistent Validation Entry Points

**Spec ID:** 0041
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-25
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U4/F6/F7/F8, the §7 full mapping and the §5–6 preserved boundaries; this round of conversation successively confirmed the full breaking migration, explicit rejection of invalid configuration, and the additional contracts R4–R7 of this document.
**Decision record:** [ADR 0087](../adr/0087-widget-api-naming-and-validation-boundaries.md).

## Requirements

### R1 — Migrate the selected type names completely, keeping no legacy aliases

| Old name | Target name |
| --- | --- |
| `CustomPaint` | `ExternalSurface` |
| `Notification` | `NotificationCard` |
| `NotificationStyle` | `NotificationCardStyle` |
| `NotificationPlacement` | `NotificationCardPlacement` |
| `Actions<A>` | `ActionHandler<A>` |
| `Shortcuts<A>` | `ShortcutMap<A>` |
| `Alignment` | `CrossAxisAlignment` |

- Migrate the corresponding module/file names, public exports, in-repo callers, macro fixtures, examples, and current API docs in step; do not maintain a compatibility layer through old aliases, old constructors, or duplicate synonymous entry points. Old names in historical ADRs/specs may keep their historical meaning; do not treat historical text as current API examples.
- `CrossAxisAlignment` still contains true Stretch; first separate positioning-only consumers from that type — do not mechanically rename and then let notification positioning keep accepting Stretch.
- Keep `MainAxisAlignment`, `PositionAlignment`, `Alignment2D`, `FlexOptions`, `FocusHandle`, the controllers, and the other component names listed in §7.1. Private ControlSurface is not made public, and no uniform Widget/Box suffix is added.

Source: review §7.1/7.5; ADR 0087 D1/D2/D5.

### R2 — Constructors, content setters, and queries express consistent operations

| Old entry | Target entry |
| --- | --- |
| `Focus::empty()` / `Focus::new(child)` | `Focus::new()` / `Focus::new().child(child)` |
| `Shortcuts::empty()` / `Shortcuts::new(child)` | `ShortcutMap::new()` / `ShortcutMap::new().child(child)` |
| `Actions::handler(handler)` / `Actions::new(child, handler)` | `ActionHandler::new(handler)` / `ActionHandler::new(handler).child(child)` |
| `Button::content(builder)` | `Button::child_with(builder)` |
| `InteractiveRegion::content(builder)` | `InteractiveRegion::child_with(builder)` |
| `LayoutObserver::from_callback(callback)` | `LayoutObserver::from_shared_callback(callback)` |
| `Padding::insets()` | `get_insets()` |
| `DecoratedBox::decoration()` | `get_decoration()` |
| `DecoratedBox::clip_behavior_value()` | `get_clip_behavior()` |
| `Row/Column/Flex::options()` | `get_options()` |
| `IconButton::label()` | `get_label()` |
| `SizedBox::requested_width/height()` | `get_requested_width/height()` |
| `ScrollController::offset()` | `requested_offset()` |

- Add ordinary `.child(...)` setters for Focus, ShortcutMap, and ActionHandler. `new` takes the required configuration rather than a child; required callbacks may still be parameters, and no empty-callback default is forced.
- `.child` and `.child_with` set the same single slot and replace each other; preserve the independent content Fiber's hooks, subscriptions, theme inheritance, and unmount behavior. Multi-slot `.children` replaces; only `push_child/extend_children` appends; an empty macro block clears, and the static/dynamic count checks follow U1.
- Widget configuration queries use `get_property`, keeping `Align::get_insets`. Controllers and immutable value types do not mechanically gain a `get_` prefix. `ScrollMetrics::offset()` remains the committed actual position, while `requested_offset()` is only the request awaiting clamping by a successful layout; preserve request normalization and the existing scroll actions.
- Keep semantically meaningful convenience entries such as `Button::text/icon`, `SizedBox::width/height`, `Separator::horizontal/vertical`, `ThemeProvider::from_signal`, and Flex `with_options`.

Source: review §7.2/7.4; ADR 0087 D3; [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md), [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md).

### R3 — Property/callback naming does not change responsibility, polarity, or event timing

| Old entry | Target entry and constraints |
| --- | --- |
| `Focus::handle/order/enabled` | `focus_handle/tab_order/focusable`; changes focus eligibility only, it is not a blanket disable of subtree input |
| `SizedBox::color` | `background`; box background only |
| `TextLabel::color`, `Icon::color` | `foreground`; foreground of the respective content |
| `TextField::color`, `EditableText::color` | `foreground`; edited text foreground, not widened to decoration or other internal elements |
| `IconButton::color_with` | `foreground_with`; interaction-state-driven icon foreground, does not override the surface background |
| `Notification::detail_color` | `NotificationCard::detail_foreground`; detail foreground only |
| `EditableText/TextField::on_changed` | `on_change`; still only reports changes to text committed by the user |
| `EditableText/TextField::on_submitted` | `on_submit`; preserves Enter modifiers and trigger timing |
| `CustomPaint::handler/schedule/ime` | `ExternalSurface::on_draw/schedule_with/ime_with`; the latter two configure a provider, they are not event notifications |

- Keep the distinction between `disabled/selected/read_only/focusable`, keep `on_click/on_activate/on_enter/on_exit/on_escape/on_unmount/on_input`, and do not mechanically merge button and ordinary interactive-region callbacks.
- `*_with` means dynamically computed configuration or content; `on_*` means an event callback. Keep `max_height_with` dynamically evaluated; it must not be frozen into a fixed height computed once.
- Keep the name `on_vertical_navigation` and change its bool to `VerticalDirection::{Up, Down}`; old true corresponds to Down and false to Up. EditableText, TextField, and the application palette use the same explicit meaning, keep IME priority and the existing trigger scope, and add no horizontal navigation.
- Ordinary UI callbacks still support UI-local captures; Rc/Arc, Send/Sync, lifetime, and call timing of shared callback handles and of external draw/input/IME/schedule providers do not mechanically change because of renaming.

Source: review F8, §7.3; ADR 0087 D3; [ADR 0081](../adr/0081-preserve-click-naming-and-standardize-disabled-builders.md), [ADR 0082](../adr/0082-ui-thread-local-widget-callbacks.md).

### R4 — Make explicit the legal domain of the remaining appearance inputs

| Input | Legal raw values |
| --- | --- |
| `Icon::size`, `IconButton::icon_size` | finite and ≥ 0; zero legal |
| `TextField::padding`, `Separator::thickness` | finite and ≥ 0; zero legal |
| ControlStyle's `min_height/horizontal_padding/icon_size/border_width/radius` | each finite and ≥ 0 |
| `Theme::spacing` | finite and ≥ 0 |
| NotificationCardStyle's `corner_radius/horizontal_padding/header_height/bottom_padding/close_width`, placement inset | each finite and ≥ 0 |
| NotificationCardStyle's `max_width` | finite and > 0 |
| U4 ordinary appearance colors, including each ControlColors state, the focus ring, notification optional colors, content/background setters, and dynamic foreground results | every RGBA component finite; no new `[0,1]` restriction is added |

- The non-negative domain rejects negative values, NaN, and ±infinity; the positive domain additionally rejects zero. Do not silently keep the previous icon size, coerce invalid field padding to zero, or wait until layout to reject an invalid thickness that could have been identified at the entry point.
- ControlStyle, Theme, notification, and named-material inputs may still be ordinary input structs; public fields, a copy that was validated once, or using only one of the states are not reasons to skip validation at the consumption boundary. Every color state contained in a style must be legal, not only the current one.
- Geometric overflow and parent-constraint problems caused by a valid combination of inputs are still handled by the existing layout errors/diagnostics and atomic geometry commit. Invalid scalars must not be masked by parent clamps, and no requirement that "the sum of all valid scalars must be representable at construction time" is added. Large yet finite valid inputs may construct successfully and then fail at the actual layout combination.
- Keep each control/business default value and valid-value semantics. Do not redo the already migrated SizedBox/Padding/Align/Flex gap, do not remove a legal unbounded parent maximum, and do not modify the existing gradient or global Color representation model.

Source: review F7, user's additional confirmation items 1/2; ADR 0087 D4/D6; the scalar/derived-geometry distinction of [ADR 0084](../adr/0084-explicit-layout-trees-and-local-configuration.md). The current fields are based on `theme.rs` and `widgets/notification.rs`.

### R5 — Unify validation responsibility; distinguish direct construction from dynamic consumption

- Direct configuration entry points that accept potentially invalid raw sizes, colors, or styles return `Result`: this covers the R4 setters, explicit ControlStyle consumption by ordinary controls, static ThemeProvider configuration, and notification style/placement/close-style/detail-foreground consumption entries. Valid parameterless default construction and child/callback attachment do not all become fallible APIs because of this.
- Equivalent entry points reuse the same legal domain and error semantics. The returned error can identify the invalid field and the rule violated; the error type name and internal helper organization are not frozen in this spec.
- A Signal-driven theme validates the complete value on every actual consumption, applying equally to the first consumption and to later updates; the dynamic foreground callback validates each result when it is consumed. An invalid value panics explicitly and identifies the field; it does not silently fall back to the old theme/default color, does not swallow the node, and does not let an invalid color enter subsequent paint consumption. Nothing here promises that the Runtime is recoverable after the panic or that it has transactional rollback.
- `ThemeProvider::from_signal` still accepts a theme Signal, and dynamic foreground is still a provider that computes a color; `Component::build` is not reworked and Result is not propagated to the Runtime just to recover errors. Recoverable direct callers handle the Result before consumption; owners of dynamic sources may check their input before publishing, but consumers cannot rely on publisher self-discipline.
- Ordinary `view!` reuses the existing final-node Result handling: Ok constructs normally, Err is a panic at the construction point carrying the error and source location; handwritten calls can recover outside the macro. Only the final node expression is handled; no attribute DSL, no automatic unwrap in intermediate chains, and no second fallible runtime are added. Migration examples must accurately show this limitation.

Source: user's additional confirmation item 2; ADR 0087 D6; ADR 0084, [spec 0038](./0038-explicit-layout-components-and-fallible-view-nodes.md).

### R6 — Materials use named inputs while keeping existing boundaries and effects

- BackdropMaterial's tone parameter is expressed by name as `offset/slope/quadratic/saturation`, and the rim parameter by name as `highlight/shade`, no longer relying on public input array positions. Named inputs and convenience entries share validation, and no second rendering algorithm is added. Internal storage/upload may continue to use compact arrays; the spelling of the new configuration type/helper is an implementation detail and must not change these field meanings.
- Preserve the existing raw-value checks and valid-value normalization in the table below; "explicitly reject invalid values" does not mean deleting the performance caps, nor does it mean turning every finite value above a cap into Err.

| Domain | Rejected | Existing cap/clamp for valid inputs |
| --- | --- | --- |
| BackdropFilter blur sigma | non-finite, negative | max 64dp |
| EdgeRefraction width / displacement | non-finite, negative | max 64dp / 32dp respectively |
| Material fill sigma | non-finite, negative | max 64dp |
| Material fill mix | non-finite | `[-1,1]` |
| tone offset / slope / quadratic | non-finite | `[-2,2]` |
| tone saturation | non-finite, negative | max 2 |
| rim highlight / shade | non-finite, negative | max 1 |

- Within the legal domain, the same parameters keep the same effective values, GPU parameter semantics, and actual effects before and after migration; negative mix/coefficients remain legal. Preserve the existing behavior of zero blur, optional material, and refraction; do not modify foreground, layout, or hit range.
- Application glass presets and the search/palette/notification recipes and lifecycle remain owned by the application; do not change parameters, globally restyle, or move product policy into the widget crate on the grounds of naming the API.

Source: review F8 and the user's additional confirmation item 3; ADR 0087 D7; the current public domain/implementation of `widgets/backdrop_filter.rs`; the material/product ownership boundary of the existing [ADR 0076](../adr/0076-native-liquid-glass-material-port.md).

### R7 — Notifications accept only positioning types; recommend root imports over a prelude

- NotificationCardPlacement's horizontal/vertical position uses the existing PositionAlignment/Alignment2D expression, and close alignment uses the existing PositionAlignment; the public positioning entry cannot accept Stretch, and no compatibility conversion is provided that accepts CrossAxisAlignment and then silently degrades.
- The original Start/Center/End keep their positional semantics; positioning-only calls in the repository that previously used Stretch to mean Start are explicitly migrated to Start. Flex's Stretch keeps its true stretch semantics. The notification's default close alignment, application positioning, size, text/close behavior, and business lifetime do not change because of the type tightening.
- Recommend importing common components, `Component/BuildCx/View`, and the existing construction/extension traits from the `harbor_widget` root; complete the root exports of common components such as Stack and TextLabel, replacing old root names per R1. Fine-grained configuration remains available in its owning module; exporting all internal types at the root is not required, and no prelude is added.
- Root re-exports do not widen visibility: internal boundaries such as U3's raw View construction and raw Signal subscription remain internal boundaries.

Source: review F6/F8, §7.1; the user's additional confirmation item 4; ADR 0087 D2/D8; [ADR 0086](../adr/0086-normalized-key-chords-and-private-runtime-control.md).

### R8 — Repository-level migration preserves business and runtime contracts

- Every public change migrates its real consumers, unit/integration tests, macro passing/failing fixtures, examples, and corresponding docs; edit shared files in coordination or integrate atomically, and do not leave a delivery state where only the library compiles while the application still calls the old API.
- Preserve U1's slot protocol, U2's return-value consumption/unconditional commit, and U3's KeyChord normalization, first-match, and public/internal boundary. Preserve the Rust expression macro's single evaluation, branch laziness, children-before-parent, deferred build, and cross-crate plus dependency-rename hygiene.
- Preserve parent-local key/type, the independent content Fiber, direct flex parent data, atomic layout, the shared interaction state machine together with the private ControlSurface, the distinct theme roles of Button/IconButton, and the controller → editing core → decorated field layering.
- Keep the application's search/palette/rail/notification/terminal bridge command sources, generation gating, focus/IME, text callback order, scroll requests versus actual position, and draw/clip order unchanged. Store/Dispatcher FIFO and the absence of automatic wake, Host/application ownership, and teardown order are unchanged.
- Verify preservation and unmount under stable identity; actual remounts caused by type/structure changes should be stated truthfully, and no lossless migration across parent levels or across old/new HMR type identity is promised. Headless/mounted tests do not replace native focus, IME, or real DLL generation teardown acceptance.

Source: review §4–7; ADR 0087 D3/D5; ADR 0079–0086.

## Solution and Seams

Continue using the existing declaration, build, layout, style, and provider mechanisms; only organize consistent naming, shared validation, and named parameters within the owning module. Static input consumption returning Result and dynamic consumption panicking do not substitute for each other.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Direct configuration | raw scalar/color/ControlStyle/Theme/card config → owning component entry | the complete R4/R6 input domain; public structs are not treated as already validated | recoverable Result and same-domain diagnostics; valid material keeps its caps |
| Dynamic configuration | Signal theme / foreground provider → build/content consumption | validation on every actual consumption without changing the original call timing | a fully valid value or an explicit field panic, never a silent fallback |
| Declaration/content | ordinary node or final Result → view!/child capability → deferred View | the existing evaluation, slot, cardinality, and Fiber ownership | consistent content semantics for handwritten and macro construction, plus the existing error exit |
| Positioning/layout | PositionAlignment/Alignment2D/card placement → layout; CrossAxisAlignment → Flex | positioning-only has no Stretch; parent constraints remain authoritative | equivalent positions, true cross-axis Stretch, derived overflow still reported as a layout error |
| Material | application glass recipe → named material input → GPU packing | old parameter meanings, validation, caps, sampling/clip order | the same valid parameters and effects; business policy is not moved |
| Public API | module/root exports → Rust calls, macro fixtures, application, docs | R1–R3/R7 target names and public visibility | the new API is usable, old entries are no longer usable, internal entries are still rejected |
| Product/lifecycle | input/notification/external draw components → application commands, host, runtime registrations | the existing focus/IME, callback order, generation/teardown contracts | migration changes neither business effects nor resource release responsibility |

Affected surface: `crates/harbor-widget/src/{theme,lib,construction,view}.rs` and the related components/exports under `widgets/` and `layout/`; the workspace, palette, tab UI, appearance, and notifications of `crates/harbor-app`; the terminal bridge/host/HMR composition calls that actually hold the old external-draw API; widget and macro tests, cross-crate fixtures, application regressions, `docs/widget/`, and current API examples. This table is an integration contract; it does not require modifying runtime or macro implementations that already satisfy the contract.

## End-to-End Tests

### E1 — New public declarations from compilation to mounting (R1–R3, R7–R8)
- **Given:** root-imported ordinary components, macro Views, Focus/ShortcutMap/ActionHandler, ExternalSurface, and real single/multi content slots.
- **When:** handwritten and view! declarations compile and mount, repeated child/child_with setting and empty-block clearing happen, and the cross-crate/dependency-rename fixtures run.
- **Then:** the target API is usable and the result/slot lifecycle is consistent; old types/methods/constructor signatures no longer compile. Leaf/count limits and the private View/Signal entries are still rejected; evaluation and build timing are unchanged.

### E2 — Invalid static configuration is recoverable; final macro nodes report clear errors (R4–R5)
- **Given:** negative numbers, NaN, ±infinity, zero, and large finite values in the size domains; zero for notification max_width; invalid components in every style color state and optional color; and ordinary solid colors that are finite but outside [0,1].
- **When:** construction goes through equivalent direct configuration entries, and the successful value or the final Result is used in real UI; the error is handled explicitly outside the macro or evaluated as the final macro node.
- **Then:** invalid input yields a field-specific Err at the consumption entry; it is not masked by a parent clamp, the old value, or zeroing. Legal zero/finite solid colors are accepted and max_width zero is rejected. It is recoverable outside the macro, and a final macro Err shows the error and source location; no automatic repair of intermediate method chains is claimed.

### E3 — Dynamic theme/colors cannot bypass validation (R4–R5, R8)
- **Given:** a mounted Signal theme and a state-driven icon foreground, with legal initial/updated values and invalid initial/subsequent values.
- **When:** the consuming theme is rebuilt, or the existing interaction path consumes a new color result.
- **Then:** every consumption validates against the same complete domain; a legal theme keeps inheritance and distinct theme roles, an invalid value panics with an explicit field, and neither the old theme/color is silently kept nor an invalid value passed into paint. The tests do not equate catch_unwind with supporting production Runtime recovery.

### E4 — A legal scalar does not imply the layout combination is representable (R4, R8)
- **Given:** individually legal large padding/style/card sizes, together with narrow, tight, wider, and unbounded parent constraints.
- **When:** successfully constructed components enter actual parent-directed layout, producing oversized combinations or relayouts.
- **Then:** valid inputs are laid out per the parent constraints; actual geometric overflow reports a layout error and does not publish invalid geometry. The existing Insets/Flex/SizedBox rules are not rewritten, and legal unbounded maxima still work.

### E5 — Named materials preserve the actual glass result (R6, R8)
- **Given:** the existing glass recipes of search, palette, and notifications, plus negative mix/coefficients, finite values above the caps, invalid negative saturation/rim, and non-finite values.
- **When:** migrating to named inputs, constructing the material, and performing real scene/GPU rendering.
- **Then:** valid inputs produce the same effective parameters/visual results as in the table, above-cap values are still truncated, and invalid values are explicitly rejected; GPU packing order, clip, foreground clarity, hit range, and product defaults are unchanged. Assertions on array values alone cannot replace actual rendering evidence.

### E6 — Notification positioning and Flex stretching are no longer conflated (R1, R4–R5, R7–R8)
- **Given:** Start/Center/End notifications with close alignment, various insets/widths, and a real Flex containing Stretch.
- **When:** window/parent constraints change after mounting, long details are displayed, close is clicked; and in parallel an incorrect Stretch input for notifications is compiled.
- **Then:** every legal placement keeps its expected geometry/hit/close path, Stretch placement fails to compile while Flex Stretch still stretches. Notification eligibility, focus, application lifetime, and generation rules are unchanged.

### E7 — Editing/commands/external draw and scrolling work end to end (R2–R3, R8)
- **Given:** the real palette/search/rail, a stable focus handle, IME, ExternalSurface's input/IME/schedule/draw providers, and a layoutable ScrollController.
- **When:** text is typed, submitted/vertically navigated, IME is handled, a nested close rejects the event first, a scroll request is issued, layout commits, and a stable rebuild/unmount occurs.
- **Then:** Up/Down meanings and IME priority are correct, and on_change/on_submit data and ordering are unchanged; color affects only the declared content. requested_offset and committed offset are correctly distinguished before and after commit; external registration/unmount and command effects do not change because of naming. Native focus/IME and real HMR teardown are evidenced separately.

## Decisions and Traceability

| Choice | Source |
| --- | --- |
| Full §7 mapping, no old aliases, repository-level migration | first user confirmation; review §7; ADR 0087 D1–D3 |
| Ordinary input structs, direct Result, dynamic per-consumption validation with panic | second user confirmation item 2; ADR 0087 D6 |
| Finite non-negative sizes, positive notification max_width, finite-only ordinary colors, derived overflow left to layout | second user confirmation item 1, previous explicit-rejection confirmation; ADR 0087 D4; ADR 0084 |
| Named tone/rim, keeping the old rejections and clamp/cap | review F8, second user confirmation item 3; current backdrop_filter.rs; ADR 0087 D7 |
| Positioning without Stretch, explicit Up/Down | review F8/§7.1/§7.3 adopted in full; ADR 0087 D2/D3 |
| Completed root imports, fine-grained configuration in owning modules, no prelude | second user confirmation item 4; ADR 0087 D8 |
| Preserve existing runtime/application/acceptance boundaries | review §5–7; ADR 0079–0086; ADR 0087 D5 |

## Verification and Limitations

- E1–E7 retain public API, real consumption and scenario coverage; rename searches, helper return values and test counts alone do not prove behavior.
- The validation matrix covers every affected scalar and color state, optional colors, dynamic initial/update consumption, finite values outside the caps, legal extremes, combination overflow, default configuration and equivalent entries without changing business defaults.
- Native focus, IME, GPU glass and real HMR generation teardown evidence remains separate from headless coverage. Named input/error/helper spellings and internal storage remain implementation choices within the fixed contract.

## Out of Scope

- U5's Host root factory/GPU errors/deadline and U6's renderer draw order.
- Redoing U1–U3 or already-delivered layout/build capabilities; changing the runtime, tree identity, layout engine, event timing, or application command policy on the grounds of naming convergence.
- Global Color/gradient model redesign, HDR policy, removing material performance caps, product appearance tuning, or new TOML configuration.
- Generic Props/Style/Container, a prelude, root-exporting all internal types, a macro attribute DSL, intermediate chained Result rewriting, Result-returning Component/runtime.
- UIA, virtual lists, full shaping, horizontal navigation, cross-parent state migration, or promises of old/new HMR type-identity compatibility.

## Delivery evidence and remaining acceptance

### Recorded evidence

Historical public API, validation, mounted application and glass-pixel evidence covered Windows/MSVC on AMD Radeon RX 7900 XT, Vulkan/DX12, 1×/1.5×/2× DPI and BGRA unorm/sRGB; not all hardware or subjective before/after visual equivalence. Ignored tests were not executed merely because the suite passed; application glass pixel cases were separately run on both backends. Local raw logs were not retained repository evidence.

### Historical native evidence boundary

The user reported native focus/IME, input ownership/switching and Windows debug HMR reload/recovery smoke as passing; the agent did not independently observe them. No separate executable/DLL identity or lifecycle trace established old-generation callbacks, captures and registrations being released **before DLL unload**. Successful reload, compilation and mounted tests do not prove that ordering. This is a historical missing-evidence limitation, not a proven defect or an active acceptance followup.
