# Card Validation, Positioning-Only Types, and CrossAxisAlignment Separation

**Ticket ID:** T0004
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R1 (Card/Alignment), R3 (detail_foreground), R4–R5 (notifications), R7 (positioning), R8; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D1/D2/D4/D6; [related file](./README.md).
**Status:** Todo

## Goal

Give NotificationCard configuration explicit validation and positioning-only types; make CrossAxisAlignment express only the true cross-axis capability, including Flex Stretch. Preserve the notification's actual position, close/scroll behavior, and business lifetime.

## Affected Surfaces

- `widgets/notification.rs` and the target card module, `layout/align.rs`, and all Alignment exports/consumers.
- FlexOptions/Row/Column/Flex, notification placement/close geometry, and the application's notifications composed with glass recipes.
- Notification/Portal/Flex/layout/API fixtures, application regressions, and current docs.

## Approach

Within the same delivery slice, first switch positioning-only consumers to PositionAlignment/Alignment2D, then migrate the true cross-axis type to CrossAxisAlignment. Migrate the notification Card/Style/Placement together with raw-input validation and real calls; close-style reuses T0001's shared ControlStyle/color validation and must not duplicate another domain.

## Dependencies and Coordination

- **Blocked by:** T0001's full validation boundary, which the notification's explicit close-style and theme/color consumption needs.
- **Blocks:** T0006 final positioning/public contract/business acceptance.
- **Coordination risks:** shares glass/notifications with T0002; shares Flex getters, exports, and docs with T0003. Positioning-only tests and migration preparation may be written first, but this ticket needs T0001 to complete, and the real declarations must be rechecked after integration.

## Acceptance

- [ ] Notification/Style/Placement are fully migrated to NotificationCard/NotificationCardStyle/NotificationCardPlacement, with modules/exports/consumers and detail_foreground synchronized and no old aliases/entries.
- [ ] All CardStyle sizes are finite and non-negative, max_width is finite and strictly positive, and the placement inset is finite and non-negative; all optional/explicit colors are finite-only. Direct consumption of the style/placement/detail foreground and close-style returns an explicit Result and does not hide an invalid scalar behind layout or defaults.
- [ ] Positioning uses PositionAlignment/Alignment2D and close alignment uses PositionAlignment; Stretch cannot be passed through the public positioning API or a compatibility conversion. Old positioning-only Stretch calls are explicitly migrated to Start.
- [ ] CrossAxisAlignment fully replaces the old Alignment and keeps only true cross-axis uses; MainAxisAlignment, PositionAlignment, and Alignment2D are not renamed, and Flex Stretch still stretches.
- [ ] Start/Center/End, the default close alignment, positions under different insets/widths/parent constraints, text wrapping/scrolling, and the actual close hit/callback match spec E6; invalid scalars are distinguished from layout overflow of a valid combination, and the default business sizes are unchanged.
- [ ] Notifications remain display components; eligibility, message meaning, deduplication, generation, dismiss history, application lifetime/auto-close, and focus policy are not moved into the component or reset.
- [ ] Real notifications/glass calls, Portal/layout/Flex regressions, public compile tests rejecting Stretch/old names, and docs are all migrated; the headless versus native/GPU evidence boundary is preserved.

## Out of Scope

A new notification manager, new business timeouts, notification appearance tuning, removing Flex Stretch, Host deadline/GPU fault handling, or re-implementing the already delivered layout API.
