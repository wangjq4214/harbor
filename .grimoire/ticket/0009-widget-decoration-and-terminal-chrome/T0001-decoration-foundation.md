# Decoration Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md)
**Status:** Todo

## Goal

Shared decoration, paint-phase, and rounded-clip contracts exist with validated logical-pixel semantics so every observable decoration slice can build on one stable foundation.

## Layers

- [ ] **Widget API and Layout:** Define public `BoxDecoration`, `BorderRadius`, `Border`, `BoxShadow`, and `ClipBehavior` value types with `Default`, fluent builders, logical-pixel semantics, and no layout contribution.
- [ ] **Fiber and Retained Scene:** Define the minimal before-child/after-child paint-phase and rounded-clip metadata contracts required by all later slices, without producing a new visible effect.
- [ ] **Widget Renderer and Frame Encoder:** Define renderer-facing normalized radius/clip data and conversion contracts; no new GPU effect is encoded in this pre-refactoring ticket.
- [ ] **Runtime Host and Terminal Bridge:** None — shared foundations must remain independent of Terminal and Host types under ADR 0011.
- [ ] **Verification:** Add unit tests for defaults, builders, finite-value validation, non-negative radius/border/blur rules, negative spread, radius normalization, and layout-neutral contracts.

## Approach

1. Add a focused public decoration module in `harbor-widget` and re-export its value types through the existing widget API surface.
2. Represent four corner radii explicitly, with uniform and per-corner constructors and deterministic normalization for boxes smaller than requested radii.
3. Reject non-finite values; require non-negative radii, border widths, and blur radii while permitting finite negative spread.
4. Add an explicit paint-phase contract capable of placing shadow/background before children and border after children without changing existing widgets' default ordering.
5. Add a rounded-clip descriptor that can be retained and DPI-converted later without depending on Terminal, winit, or Host resources.
6. Cover value equality and stable defaults so retained-scene change detection can compare decoration state deterministically.

## Blocked by

- (none)

## Blocks

- T0002 — Concrete `DecoratedBox` fill and border consume all shared values and paint phases.
- T0003 — Shadow rendering consumes `BoxShadow` and normalized geometry.
- T0004 — Rounded clipping consumes `ClipBehavior` and clip descriptors.
- T0005 — External-draw clipping consumes the same retained clip contract.
- T0006 — The Host preset is expressed entirely through these public values.

## Acceptance

- [ ] All five public value types can be constructed through `Default` and fluent builders without exposing Terminal types.
- [ ] Uniform and per-corner radii normalize deterministically within finite box extents.
- [ ] Non-finite values and negative radius, width, or blur are rejected by the public contract; finite negative spread is accepted.
- [ ] Default `BoxDecoration` has no color, border, radius, or shadows, and default `ClipBehavior` is `None`.
- [ ] Foundation tests pass without introducing any observable rendering or layout change to existing widgets.
- [ ] `harbor-widget` remains independent of `harbor-terminal` and terminal rendering types.

## Out of Scope

- Rendering backgrounds, borders, shadows, or clips.
- A concrete `DecoratedBox` widget.
- Inner shadows, gradients, images, arbitrary paths, or per-edge borders.
- Terminal root composition or any Host behavior change.
