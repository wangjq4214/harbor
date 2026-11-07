# Parent-Directed Flex Layout

`harbor-widget` measures child subtrees through an indexed parent-directed boundary,
then validates and commits the complete candidate geometry. Layout never builds
Components, invokes external draw callbacks, or publishes trial rectangles.

## Fixed rail and remaining content

```rust
use harbor_widget::{CrossAxisAlignment, ConstrainedBox, Expanded, Row, Separator};
use harbor_widget::widgets::external_surface::ExternalSurface;
use harbor_widget::widgets::text_label::TextLabel;

let root = Row::new()
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .push_child(ConstrainedBox::new()
        .min_width(200.0).max_width(200.0)
        .child(TextLabel::new("Rail")))
    .push_child(Separator::vertical())
    .push_child(Expanded::new().child(ExternalSurface::new(1)));
```

At 1000dp width the allocations are 200dp, 1dp, and 799dp. Replacing both rail
bounds with 56dp leaves 943dp. `max_width` alone caps the content; it does not
request that width. Local bounds are enforced within the parent interval, so a
fixed rail cannot override a smaller parent maximum. The legacy
`BoxConstraints::constrain` min-wins behavior is unchanged.

`Row` and `Column` retain their own widget identities and background painting;
both use the same axis-neutral Flex engine. With no flexible children they
shrink-wrap subject to parent bounds. With flexible children and a finite main
maximum they fill that maximum, even under loose root constraints.

## Allocation policy

- Inflexible children are measured first with a loose main minimum and the parent
  main cap after explicit gaps. Multiple such children can still overflow.
- Positive integer factors divide the remaining finite main extent. Expanded and
  Spacer use tight shares; Flexible defaults to loose fit. Loose under-consumption
  is not redistributed. Alignment uses actual occupied space, not unused slots.
- Start, center, end, space-between, space-around and space-evenly main alignment
  add nonnegative free space to the minimum explicit gap. Overflow starts at zero.
  Main-axis overflow up to `f32::EPSILON * main_extent` is ignored as diagnostic
  rounding noise; this tolerance never changes allocations or placement geometry.
- Cross alignment supports start, center, end and true constraint-based stretch.
  When the cross maximum is unbounded, stretch derives a finite natural extent
  and performs at most one corrective measurement per affected child.
- Unbounded main constraints use finite content-driven layout and report
  `UnboundedFlex`; tight flex falls back to loose. Spacer contributes zero main
  extent. Fill-style ExternalSurface uses finite natural content/minimum on an
  unbounded axis rather than returning infinity.
- Separator thickness defaults to **1 logical pixel**, not one physical pixel.
  Its finite long-axis maximum is filled; an unbounded long axis defaults to zero
  unless bounded constraints supply an explicit extent.

## Optional wrapping

`Row::wrap(true)`, `Column::wrap(true)` and `Flex::wrap(true)` opt into
additional runs at the available main-axis cap. The equivalent local setting is
`FlexOptions { wrap: true, ..Default::default() }`; the default remains a single
run. Children are measured using the full main-axis cap, not a guessed breakpoint
or application-maintained text widths.

Main-axis alignment is applied separately within each run. Cross-axis start,
center and end apply within each run's natural cross extent. Stretch retains the
existing constraint policy: a finite cross maximum is supplied on the first
measurement, so each run can fill that cap and multiple runs can overflow it.
With an unbounded cross maximum, stretch derives each run's natural finite cross
extent and corrects its children once. This avoids introducing repeated intrinsic
stretch correction in bounded nested layouts.

Runs advance from the cross-axis start, with the configured `gap` both between
children and between runs. As with single-run flex, alignment needs
parent-imposed minimum space to distribute; opting into wrapping does not force
a content-sized group to fill the available width.

Flexible children have zero basis when choosing run breaks; their factors divide
the remaining main extent of their own run using the existing fit and rounding
rules. With an unbounded main axis there is no wrapping, and unbounded-flex
diagnostics remain unchanged. Stretch uses provisional measurements followed by
at most one correction per child; corrections do not regroup runs or redistribute
shares. Overflow stays diagnostic rather than implicitly clipping content.

Applications choose whether to wrap, alignment, spacing and child order. No
action-group policy or button semantics are built into the layout engine.

## Parent data and errors

Flex factor and fit are typed metadata on the immediate child wrapper.
`Flexible(Padding(content))` works; `Padding(Flexible(content))` does not tunnel
metadata through Padding. Deferred child construction preserves hooks and wrapper
identity. Invalid parent use and conflicting immediate flexible wrappers produce
diagnostics and use ordinary wrapper layout rather than guessing at allocations.

Repeated identical child measurement requests reuse a completed result. Each child
may receive at most two distinct requests in one parent invocation. A request
includes constraints and whether it is a provisional natural probe or final
measurement. Actual node measurements share a pass-wide budget of **8 times the
reachable node count**; this bounds nested correction work as well as local
remeasurement. It is a safety limit, not a performance guarantee or an
intrinsic/fixed-point solver.

During natural probes, nested Flex stretch containers defer their own unbounded
cross-axis corrections. Once the final ancestor derives a finite cross extent, it
remeasures every probed child with final constraints. This avoids quadratic
correction walks in ordinary nested stretch trees. Provisional subtrees cannot be
committed, including when a child's natural size already matches its final size.

Invalid constraints, non-finite geometry, invalid/missing/duplicate placements,
stale children, recursion and exceeded work limits abort the candidate pass.
All absolute rectangle edges are validated before any rectangle is committed.
Previous committed geometry remains available; nodes without previous geometry
receive a zero-size, non-hit-testable fallback. Fiber accessors expose the last
layout error and committed diagnostics. A new successful pass clears obsolete
errors and diagnostics; failure alone does not continuously schedule retries.

## Painting, input and resize

Overflow is diagnostic, not an implicit clip or a negative flexible share.
A child's rectangle is its actual measured allocation. Hit testing uses that
allocation and explicit ancestor clips, not a loose child's unused share or a
shadow. Right/bottom edges are excluded and zero-area allocations cannot be hit.
Visible overflow can be targeted outside an unclipped parent's rectangle.

Runtime flushes pending layout before routing the next input event. Viewport and
text-metric changes relayout the whole root without rebuilding Components; build
invalidations conservatively rebuild and relayout the root so sibling allocation
cannot become stale. Layout-only updates preserve retained scene IDs and external
registrations. DPI conversion stays at the renderer boundary; layout remains in
unrounded logical pixels.

Responsive breakpoint decisions and embedded-content resizing remain application responsibilities. Use `LayoutObserver` to observe committed allocation. Positioned children are outside this Flex contract.
