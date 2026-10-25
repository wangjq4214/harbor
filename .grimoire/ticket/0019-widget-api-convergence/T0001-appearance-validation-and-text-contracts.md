# Appearance Validation, Color Vocabulary, and Text Direction Contracts

**Ticket ID:** T0001
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R2 (IconButton getter), R3 (colors/text/direction), R4–R5, R8; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D3/D4/D6; [related file](./README.md).
**Status:** Todo

## Goal

Make invalid direct input to theme, ControlStyle, icon, field, separator, and ordinary content/background colors recoverably rejected, and prevent dynamic themes/foreground from bypassing validation; migrate the color, text-callback, and direction APIs in step without changing business meaning.

## Affected Surfaces

- Widget `theme.rs`, `widgets/{theme,control_surface,button,interactive_region,icon,icon_button,text_field,editable_text,text_label,sized_box,separator}.rs`, plus any necessary shared validation/exports.
- Calls in the real application's appearance, palette, workspace, tab UI, and related notification recipes that need migration because the ordinary ControlStyle entry now returns Result.
- The related widget/application tests, compile fixtures, and current docs examples. The notification card's own raw configuration/rename is owned by T0004.

## Approach

Implement shared scalar/solid-color/ControlStyle/Theme validation in the first batch of real consumers; do not build a standalone preparatory refactor with no consumers. Ordinary structs remain inputs and the consumption boundary owns complete validation; do not change `Component::build` to return Result.

Static entries return Result, while Signal themes and dynamic foreground are checked on every actual consumption and panic as the spec specifies. Keep the existing final-node macro handling rather than automatically rewriting intermediate chains.

Migrate the appearance vocabulary involved in validation together with its callers, avoiding a state where two color setter sets are kept first and cleaned up later.

## Dependencies and Coordination

- **Blocked by:** none.
- **Blocks:** T0004 full pass, because its notification close-style/theme consumption must reuse this ticket's unified validation; T0006 final combined acceptance.
- **Coordination risks:** T0003 changes child_with/getters in the same files; T0004 modifies notification calls. Shared rules/error behavior are already fixed by the spec; coordinate export and application declaration edits and do not add semantic blocking to other independent tickets because of them.

## Acceptance

- [ ] Icon size/icon_size, field padding, separator thickness, all ControlStyle sizes, and Theme spacing accept finite non-negative values per R4 and reject negative numbers/NaN/±infinity; zero stays legal. Errors no longer fall back to the old value/zero and are not deferred to layout.
- [ ] All state colors, the focus ring, and U4 ordinary color entries fully validate as finite RGBA; solid colors that are finite but outside [0,1] can still be input. Neither the gradient nor the global Color model changes.
- [ ] Static themes and explicit style consumption by ordinary controls return field/rule-specific Result, and all equivalent entries use the same semantics. Explicit styles and inherited themes keep distinct Button/IconButton roles.
- [ ] Both the initial and subsequent consumption of a Signal theme, and every consumed dynamic foreground result, are validated; invalid values panic with an explicit field, do not fall back, and make no post-panic recovery promise. The providers' original evaluation timing and UI-local captures are preserved.
- [ ] Fully migrate `SizedBox::color → background`, `color → foreground` for TextLabel/Icon/EditableText/TextField, and IconButton `color_with → foreground_with` plus `label → get_label`, without widening color scope; old entries are no longer usable.
- [ ] EditableText/TextField `on_change/on_submit` and the shared `VerticalDirection::{Up,Down}` are wired through the real palette, with old true→Down/false→Up; IME priority, Enter modifiers, and editing callback data/timing are unchanged.
- [ ] Valid large scalars and actual derived layout overflow are clearly distinguished, and parent clamps do not mask invalid input; default values, unbounded parent constraints, and existing layout scalar rules are unchanged.
- [ ] Cover this ticket's paths for spec E2–E4 and E7 plus compilation of the new/old public APIs; real consumers, tests, macro fixtures, and docs all complete their signature migration, and the existing U1–U3 regression contracts are preserved.

## Out of Scope

The notification card's own configuration/positioning migration (T0004), named material configuration (T0002), the slot/query migration of the general naming ticket (T0003), redesigning Runtime error propagation, product tuning, and U5/U6.
