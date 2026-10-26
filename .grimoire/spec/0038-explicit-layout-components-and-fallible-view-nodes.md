# Explicit Layout Components and Fallible View Nodes

**Spec ID:** 0038
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-24
**Sources:** historical UI/API review F5, including its related F7/F8 layout concerns; refinement conversation selecting targeted breaking migration, plain input configurations with fallible component construction (option A), automatic final-node Result handling in ordinary `view!`, construction-site panic on Err.
**Decision record:** [ADR 0084](../adr/0084-explicit-layout-trees-and-local-configuration.md).

## Requirements

### R1 — Expose layout through an explicit component tree

- Use named parent components for sizing, padding, alignment, decoration and flex participation. Do not add general `padding/width/height/size/constraints/align/decorated/expanded` wrapping methods to ComponentExt.
- Retain `.keyed(...)` and existing useful component-local settings, including Button callbacks and TextField editing configuration. This is not a removal of all builders.
- Keep ordinary Rust node expressions, `=>` child blocks and child-list `for/if/match`. Do not introduce a widget-name whitelist, macro property language, universal Style/Container, new Element runtime or separate layout engine.
- Preserve the distinction between external Padding and a TextField's internal padding. Keep product composition and business policy in the application.

Sources: F5.1–F5.4; ADR 0084 D1/D4/D6; [ADR 0032](../adr/0032-view-macro-rust-expression-grammar.md), as amended by [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md).

### R2 — Make SizedBox an optional SingleChild size component

- Retain the `SizedBox::new(Size)` construction name and its two-axis placeholder purpose. Provide `SizedBox::width(f32)` and `SizedBox::height(f32)` for exactly one requested axis. These raw-numeric entry points return `Result<SizedBox, E>` under R5; retaining the name does not preserve the old infallible signature.
- Store unspecified axes distinctly from zero and infinity. Replace the public Size-only `size` field with private optional-axis configuration and read-only inspection that distinguishes specified from unspecified axes. Migrate direct field access and construction; do not retain a misleading compatibility field.
- Provide `.child(value)` and the shared `WithChildren<Capability = SingleChild>` capability. A missing child remains valid. Repeated setters replace, a macro block replaces the staged slot, and an empty block clears it under [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md).
- For each specified axis, clamp the requested extent to the validated authoritative parent interval and measure the child with that tight extent. On an unspecified axis, inherit the parent interval and use the child's measured extent. With no child, use zero natural extent on unspecified axes and then satisfy the parent's minima.
- Place the child at the local origin. Sizing adds no focus stop, event handling or implicit clipping. Retain `.color(...)` drawing over the actual allocated rectangle.
- A requested width of 440 yields 300 under a parent maximum of 300, 500 under a tight parent width of 500, and 440 when loose parent bounds permit it. Width-only construction must not fix the child's height to zero or infinity.
- Constraints or content changing after mount must produce correct new geometry through the existing layout path, including layout-only changes without rebuilding components.

Sources: F5.4 B, F5.8; ADR 0084 D2; [parent-directed layout contract](../../docs/flex-layout.md).

### R3 — Share named Insets and two-dimensional alignment

- Use one ordinary `Insets` input value for Padding and Align, with named `top`, `right`, `bottom`, `left` fields and `Insets::all(value)`. Building this plain value is not successful component validation; consuming components apply R5.
- Replace Padding's four positional constructor arguments with `Padding::new(Insets) -> Result<Padding, E>`. Any retained uniform-padding convenience uses the same validation and layout semantics, not a second implementation.
- Replace Align's one-dimensional constructor plus vertical override with `Align::new(Alignment2D)`. Provide the common `TOP_RIGHT` and `CENTER` positioning values and express horizontal/vertical placement without accepting Stretch. Internal representation and additional convenience constant names are not prescribed.
- Align's inset setting consumes the shared Insets value and is fallible. Preserve its existing reduction of child constraints and available positioning bounds, finite bounded-axis fill, natural extent on unbounded axes, background behavior and non-hit-testable alignment space.
- Do not mechanically replace Align's insets with `Padding => Align` or `Align => Padding`; equivalence must be established under tight, loose and unbounded constraints, not assumed from one screenshot.
- Padding retains its single-slot constraint deflation, child offset, own-size and background responsibilities. Zero/asymmetric insets, excessive insets relative to a tiny parent and the child-free case remain observable cases.
- Row/Column keep real cross-axis Stretch. Do not move Stretch into a positioning-only value or remove it from flex layout.

Sources: F5.4 C; F8's Insets/Alignment2D recommendations; ADR 0084 D3; current Padding/Align behavior.

### R4 — Concentrate Flex settings without duplicating behavior

- Provide ordinary `FlexOptions` with local `gap`, `main_axis_alignment` and `cross_axis_alignment` settings. Row and Column expose `with_options(options)` returning `Result` after validation; Flex has the equivalent options entry point while retaining an explicit axis choice.
- Default options preserve current Flex defaults: zero gap, Start main-axis alignment and Start cross-axis alignment. Row and Column preserve their own widget identities and use the same axis-neutral Flex engine as Flex.
- Options and retained short builders use the same effective configuration and validation. Relevant raw-numeric setters return Result; valid equivalent options/builder inputs yield equivalent constraints, allocation, placement and diagnostics.
- Keep axis in component construction and children in the shared slot protocol. Options do not contain padding, margin, key, event callbacks, business themes or a general-purpose style bag.
- Preserve existing main-axis distribution, positive flex factor/fit behavior, cross-axis Stretch, unbounded-main fallback, overflow diagnostics and measurement budgets. Expanded/Flexible metadata remains immediate-parent data and does not tunnel through Padding, SizedBox or other ordinary wrappers.

Sources: F5.4 D/E; ADR 0084 D4/D6; current Flex defaults and [parent-directed layout contract](../../docs/flex-layout.md).

### R5 — Validate raw configuration at the component boundary

- Insets and FlexOptions are ordinary named inputs, not opaque validated configuration objects callers must manufacture before declaring UI. Relevant component constructors and raw-numeric setters validate input and return Result; handwritten callers may propagate, match or otherwise recover from Err.
- Requested fixed dimensions, gaps and each individual Insets side must be finite and nonnegative. Zero is valid. NaN, positive infinity, negative infinity and values below zero return Err at the component boundary; invalid requested sizes must not be hidden by later parent clamping.
- All equivalent input paths share these rules and errors. Do not silently ignore invalid input, normalize it to zero or expose an apparently successful component that postpones an invalid scalar until layout. This is targeted F7 work for R2–R4, not conversion of every existing widget numeric API.
- Scalar validity and derived geometry validity are separate. Individually valid finite values remain valid configuration even if their combination later overflows. In particular, opposing inset sides may each be finite and nonnegative while their sum exceeds representable geometry; constructing the component succeeds and the layout combination reports an error. Do not add a construction-time sum cap. Child-count-dependent gap totals and child/parent geometry combinations also remain layout concerns.
- Keep requested extents distinct from valid unbounded parent maxima. Existing BoxConstraints validation allows positive infinity as a maximum, not as a measured size or a requested fixed dimension. Single-axis omission uses absence, not zero or infinity.
- Parent-constraint errors and derived geometry overflow remain layout errors/diagnostics through the existing atomic geometry-commit contract; failed layout does not commit non-finite geometry. A fallible constructor does not replace layout validation or convert all layout failures into construction panics.
- Public error type names, storage helpers and read-only accessor spellings are implementation details, provided errors remain recoverable outside `view!` and carry the information needed by R6.

| Input or event | Required outcome |
| --- | --- |
| Zero or another finite, nonnegative requested dimension/gap/inset side | Valid scalar configuration |
| NaN, either infinity or a negative requested dimension/gap/inset side | Component-boundary Err; ordinary macro handling follows R6 |
| Width-only/height-only omitted opposite axis | Unspecified, not an invalid sentinel value |
| Positive-infinite maximum in otherwise valid parent constraints | Existing unbounded-parent behavior, not constructor rejection |
| Finite valid opposing inset sides or gap values whose layout combination overflows | Successful scalar construction; layout error without committing invalid geometry |

Sources: accepted option A and explicit numeric-policy approval; F5.8/F7; ADR 0084 D5/D8; current BoxConstraints and Flex contracts.

### R6 — Let ordinary view! handle final fallible node results without question marks

- In both root and child positions, accept an ordinary component/already-built View or a final `Result` whose Ok value is such a node. Support both value nodes and nodes receiving a `=>` child block, including selected conditional/match branches and loop-produced children.
- Resolve Ok before attaching that node's child block and converting to the deferred description. Use the same capability/cardinality protocol as for the underlying ordinary component; Result does not turn a leaf into a parent or weaken static SingleChild checking.
- Err causes a clear construction-site panic that includes the configuration error and the originating node's source location. Ordinary `view!` still returns View; it does not propagate Result to Component/runtime. Do not silently omit the node, substitute defaults or turn invalid input into an empty child.
- Recoverable callers handle Result outside the macro and provide a valid ordinary node. The automatic behavior is deliberately limited to the final node-expression result: it does not insert unwraps within arbitrary Rust expressions, configuration fields, helper bodies or intermediate method-chain calls.
- Normal explicit-tree declarations using the selected constructors/options require neither `?` nor manual unwrap at each fallible node. Prefer local options to long chains. For additional methods on an already fallible constructor, callers may first handle its Result outside the macro; automatic intermediate-chain rewriting is not promised.
- Preserve single evaluation, sibling order, children-before-parent expression evaluation, branch laziness, ownership moves and deferred Component build timing. Do not inspect an expression by executing it twice, eagerly build successful nodes or evaluate an unselected Err branch. Existing View values and explicit keys retain their ordinary conversion behavior.
- Support external component types through the same construction mechanism; no built-in name recognition. Keep cross-crate and renamed-dependency macro hygiene. The internal trait/helper design is not part of this contract.

Sources: explicit refinement approvals of automatic final-node handling, panic/source-location reporting and recovery outside the macro; ADR 0084 D7; ADRs 0032/0079/0080.

### R7 — Migrate declarations without changing application behavior

- Migrate affected production callers, tests, compile fixtures, exports, examples and widget documentation together. Explain fallible handwritten construction, question-mark-free macro nodes, slot semantics, explicit wrapper order and supported component-local builders. No general F6 prelude or constructor cleanup is required by this scope.
- Demonstrate the new declaration style in `workspace.rs::search_bar` and `tab_view_ui.rs::tab_rail`: the fixed status slot, fixed search-panel size, rail width, named insets, two-dimensional positioning and multi-property Row/Column layout remain readable in the explicit tree.
- Replace only semantically fixed extents. A maximum alone remains a cap; fixed width plus maximum height is not fixed width/height. Keep real range constraints and font-metric-dependent `max_height_with`; do not freeze the resolver into a one-time height.
- Preserve Dispatcher command sources, popup generation validation, controllers, focus handles, editing-event order, search result/status behavior, rail scrolling and descendant close-button first refusal.
- Keep GlassAppearance's filter/decoration/clip preset and construction timing. Explicit layout migration does not authorize decomposing its business invariant or changing theme/generation lifetime.
- Replace SizedBox-based leaf-failure fixtures with a real leaf such as TextLabel; add SizedBox SingleChild passing cases, excess-child compile failures and empty-block clearing. True leaves must still reject every attached block, including an empty block.

Sources: F5.3/F5.5/F5.7/F5.8; ADR 0084 D5/D6; inspected application declarations.

### R8 — Preserve structure-dependent runtime ownership

- Keep parent-local key/type reconciliation, independent state-driven content-slot hooks/subscriptions, inherited themes and standard unmount behavior under ADRs 0079/0080. No GlobalKey or cross-parent state-preservation promise is introduced.
- Verify stable keyed insertion/reorder, state/focus retention, slot replacement/clearing and external draw/IME registration cleanup. Identify remounts caused by a deliberate wrapper/type change rather than claiming keys make all tree rewrites equivalent.
- Preserve declared decoration/paint/clip order and hit-test ranges. Do not optimize explicit wrappers into a canonical order that erases their differences. No new logical Tab stop is introduced by a pure sizing/layout wrapper.
- Existing native focus/IME and actual HMR generation teardown obligations remain separate from headless evidence.

Sources: F5.6/F5.8; ADR 0084 D6; ADRs 0079/0080 and the existing layout/runtime contracts.

## Solution and Seams

Use the existing deferred description, slot-capability and parent-directed measurement model. Extend the particular layout components and add shared local configuration rather than another runtime. Add automatic final-node Result resolution at the macro/construction boundary; that requirement does not dictate Rust trait encoding or a new parser language.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Numeric configuration | Insets/FlexOptions/raw extents → component constructors/setters | Ordinary inputs and one shared validity/error contract | Recoverable Result before successful component construction |
| Fallible description | Rust node expression → `view!` construction support | Ordinary node or final Result; originating source location | Ok normalization or explicit construction-site panic, without repeated `?` |
| Slot attachment | Normalized component → capability protocol → deferred View | Real SingleChild/MultiChild capability and existing static/dynamic count rules | Equivalent attach/replace/clear semantics for plain and fallible nodes |
| Measurement | Layout parent → SizedBox/Padding/Align/Flex → immediate child measurer | Valid authoritative bounds and current text metrics | Finite validated child allocations/placements, no eager build or trial geometry publication |
| Flex parent data | Expanded/Flexible → direct Flex/Row/Column parent | Existing immediate-child metadata | Existing share allocation or existing misuse diagnostics, not metadata tunneling |
| Retained ownership | Deferred descriptions → reconciler/hooks/focus/external registrations | Existing parent-local key/type and slot boundaries | Preserved lifetime where identity is stable; explicit remounts/cleanup where it is not |
| Application composition | Search/rail/glass declarations → layout/control widgets and Dispatcher | Existing command source/generation, focus and product layering | New declarations without changed business intent, input order or material responsibility |

Relevant surfaces: `crates/harbor-widget/src/{construction,lib,view}.rs`, `layout/`, `widgets/{sized_box,padding,align,flex,row,column,constrained_box,flexible}.rs`; `crates/harbor-widget-macros/src/` and `tests/ui/`; application `workspace.rs`, `tab_view_ui.rs`, `appearance.rs`; layout, attachment, macro, lifecycle and application regression tests; `docs/widget/` and affected READMEs. This is an integration map, not an implementation sequence or mandatory edit list.

### Target declaration

The following illustrates the selected public API direction. It intentionally has no `?` at node boundaries; imports and complete application state are omitted.

```rust
view! {
    Align::new(Alignment2D::TOP_RIGHT) => {
        SizedBox::width(440.0) => {
            Padding::new(Insets::all(8.0)) => {
                Column::with_options(FlexOptions {
                    gap: 8.0,
                    ..Default::default()
                }) => {
                    TextLabel::new("Search");
                    Row::with_options(FlexOptions {
                        gap: 4.0,
                        cross_axis_alignment: Alignment::Center,
                        ..Default::default()
                    }) => {
                        Expanded::new() => {
                            TextField::new(controller.clone()).disabled(!enabled);
                        }
                        Button::text("Close").on_click(on_close);
                    }
                }
            }
        }
    }
}
```

## End-to-End Tests

### E1 — Mount and relayout sized content under authoritative parents (R2, R8)
- **Given:** Mounted width-only, height-only and two-axis SizedBoxes, with and without children, including color-bearing placeholders.
- **When:** Parent bounds change between permissive, smaller-maximum, tight-larger and unbounded-axis cases, and content natural sizes change.
- **Then:** Specified axes are tight after parent enforcement, unspecified axes follow child/minimum rules, children remain at the origin and colors cover committed rectangles. Width 440 produces the R2 examples. Layout-only changes update geometry without changing retained identity or registering new focus/input behavior.

### E2 — Construct, replace and clear a real SizedBox slot (R2, R6, R7)
- **Given:** Ordinary and final-Ok SizedBoxes, including staged child content, in root/child positions and handwritten attachment.
- **When:** A setter or block replaces the slot, or an empty block clears it, and the tree mounts/renders.
- **Then:** Exactly the selected content remains, with equivalent key, geometry and scenes. Static overcount fails compilation; dynamic overcount returns the existing recoverable child error outside the macro and panics at construction inside it. A genuine leaf still rejects attached blocks even through final Ok.

### E3 — Declare fallible nodes without punctuation and preserve evaluation (R6)
- **Given:** Mixed ordinary, fallible component and fallible already-built View nodes, external component types, side-effect counters and selected/unselected conditional branches.
- **When:** Root/value/parent-block nodes, nested nodes and loop/if/match children are described and then mounted.
- **Then:** Valid nodes require no per-node `?`/unwrap, descriptions evaluate once in the established order, unselected branches are not evaluated and builds remain deferred. Existing View keys and underlying attachment capabilities are preserved. Cross-crate and renamed-dependency fixtures compile.

### E4 — Reject a bad declaration visibly or recover explicitly (R5, R6)
- **Given:** A constructor returning Err for invalid configuration, used as a value node or a node with a child block at root/child positions.
- **When:** Ordinary `view!` evaluates that node.
- **Then:** The construction-site panic contains the actual configuration error and originating source location; there is no silently missing/default node. Side-effect traces preserve children-before-parent and stop at the failing point, without a duplicate node-expression evaluation.
- **Also:** Handle the same Result outside `view!` and pass a valid selected component; no automatic panic is imposed on that recovery path. An unselected Err branch does not panic. An invalid intermediate Rust chain is not made valid by hidden macro rewriting.

### E5 — Preserve named inset/alignment and explicit wrapper order (R3, R8)
- **Given:** Equivalent old/new asymmetric and uniform inset configurations, top-right/center positioning, and deliberately different Padding/decoration/clip wrapper orders.
- **When:** Actual runtime layout and rendering run under loose, tight, tiny-parent and unbounded-axis constraints, including empty content.
- **Then:** Equivalent valid configurations preserve child constraints/positions, own/background rectangles and alignment-space hit behavior. Distinct wrapper orders retain their specified paint, clip and hit-test differences; the implementation does not merge them based only on outer size.

### E6 — Keep options, builders and flex parent data equivalent (R4)
- **Given:** Equivalent Row/Column/Flex options and short-builder configurations, with no flex, Expanded/Flexible children, cross-axis Stretch and mixed natural content.
- **When:** Runtime measures bounded/unbounded main and cross axes, changes constraints and encounters overflow or a wrongly nested flexible child.
- **Then:** Valid options and builders agree on allocation, placement and diagnostics; defaults are unchanged, correct direct-parent flex works and wrong wrapper nesting retains existing diagnostics. Numeric input failures use the same boundary validation. Existing correction/work budgets remain in force.

### E7 — Keep search, rail and glass behavior through migration (R7, R8)
- **Given:** The migrated real search panel, status slot, rail and nested close controls.
- **When:** Text/status changes, windows narrow/widen, Enter/Shift+Enter/Escape run, tabs insert/reorder/close, scroll changes and stale popup callbacks attempt to dispatch.
- **Then:** Fixed slots and correct mixed/range constraints remain correct; status changes do not shift the editor/buttons, search commands retain source/generation checks and event order, close does not activate the parent row, rail/focus behavior stays intact and glass filter/clip/paint ordering is unchanged.

### E8 — Preserve retained state and release obsolete resources (R6, R8)
- **Given:** Keyed stateful children and state-driven slots with observable subscriptions, focus handles, theme inheritance and external registrations.
- **When:** Valid ordinary/fallible declarations rebuild and reorder stably, slots replace/clear, a deliberate wrapper change remounts content and roots unmount.
- **Then:** Stable identities retain state/focus, explicit remounts are identified, removed resources/subscriptions stop participating and no extra Tab stop appears. Native IME/focus and actual supported HMR generation release require their own evidence rather than inferred headless success.

### E9 — Separate invalid scalars from valid inputs whose geometry overflows (R5, R6)
- **Given:** SizedBox requests, Padding/Align Insets and Flex gaps supplied through equivalent ordinary APIs/options, including zero, NaN, both infinities, negative values and large finite nonnegative values.
- **When:** Components are constructed, then successful values are mounted and actually laid out.
- **Then:** Zero and other finite nonnegative scalars construct successfully; NaN/infinite/negative scalars return Err before mounting, or produce the R6 source-located panic when used as final macro nodes. Parent clamping must not hide invalid requested input.
- **Also:** Individually valid opposing inset sides such as two f32::MAX values are not rejected at construction; their unrepresentable layout sum reports a layout error. Child-count-dependent total-gap or other derived geometry overflow likewise fails layout without publishing invalid rectangles. Valid unbounded parent maxima retain the established finite-content layout behavior.

## Decisions and Traceability

| Material choice | Established source |
| --- | --- |
| Explicit component nesting; no universal wrapping/style/Container API | Revised F5.1/F5.4 and accepted recommendation; ADR 0084 D1 |
| SizedBox optional SingleChild, optional axes, private size configuration/read-only access | F5.4 B and accepted recommendation; ADR 0084 D2 |
| Shared Insets, positioning-only Alignment2D, preserved Align semantics and flex Stretch | F5.4 C/F8 and accepted recommendation; ADR 0084 D3 |
| Local shared FlexOptions, existing defaults/engine/identity | F5.4 D, inspected Flex source, accepted recommendation; ADR 0084 D4 |
| Plain configs, Result-returning numeric component APIs, no indefinite compatibility layer | User's option A approval and scope approval; ADR 0084 D5 |
| Finite, nonnegative requested dimensions/gaps/inset sides; derived combination overflow remains a layout error | Explicit numeric-policy approval; ADR 0084 D8 |
| Final-node-only automatic Result handling; ordinary macro Err panic with error/location; recover outside macro | Explicit user request and subsequent approval; ADR 0084 D7 |
| No intermediate-chain rewriting; preserve evaluation and deferred construction | Explicit accepted handling boundary; ADRs 0032/0079/0080/0084 |
| Preserve direct flex parent data, application command/focus/material boundaries and lifecycle | F5.4 E/F5.5/F5.6/F5.8; existing layout/runtime contracts; ADR 0084 D6 |

## Verification and Limitations

- E1–E9 retain public compile fixtures, real mounted parent-directed layout, scene/hit testing, keyed/slot lifecycle and application regression coverage. A helper returning a desired Size is not sufficient evidence of child-constraint correctness.
- The R5/E9 matrix distinguishes scalar boundary errors, valid unbounded parent maxima and later derived-geometry overflow; equivalent options/builders share validation and macro panics carry the source location.
- Passing/failing fixtures retain ordinary/final-Result roots/children, parent/value forms, SingleChild replacement/clearing, genuine leaves, external types, dependency renaming, evaluation order and macro hygiene.
- Actual GPU layout/clip, native focus/IME and supported real HMR evidence remains distinct from headless coverage. The macro convenience does not promise host recovery, rollback or continued execution after its construction-site panic.
- No additional timing/performance budgets, product dimensions or runtime ownership policies are established. Error type/accessor/helper spellings and Rust implementation mechanisms remain implementation choices within this contract.

## Out of Scope

- Repository-wide F6 constructor/export/prelude cleanup, unrelated F7 numeric APIs, or non-layout F8 navigation/material type redesign.
- Generic layout extension methods, a universal Style/Container, a new property DSL, arbitrary method-chain Result rewriting, a new fallible macro variant or a Result-returning Component/runtime redesign.
- GlobalKey/cross-parent state migration, parent-data tunneling, a new layout engine, implicit wrapper reordering, input/focus policy changes or business-command ownership changes.
- Flutter's complete SizedBox convenience family, new synonym size wrappers, forced Props/Options for every widget, glass preset decomposition or changes to its eager/deferred construction timing.
