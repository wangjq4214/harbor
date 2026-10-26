# Non-Rendering Host Deadlines

**Ticket ID:** T0002
**Source:** [spec 0042](../../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) R2/R6, E2; [ADR 0088](../../adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md) D2; review F15.
**Status:** Todo

## Goal

A non-drawable Host preserves future application wake deadlines without acquiring/submitting a frame, spinning render animation, or losing normal scheduling after restoration.

## Affected Surfaces

- `crates/harbor-widget/src/winit/{scheduler,presenter,host}.rs` and relevant wait/effect propagation.
- Scheduler/adapter regressions such as `crates/harbor-widget/tests/winit_qa.rs`, public/native wait contracts and docs.
- Actual cross-window wait arbitration in `src/{shell,dialog}.rs` where affected; retain the application's separate notification/business timer ownership.

## Approach

Separate future Host wake demand from render eligibility in the existing scheduling path. Preserve drawable Runtime/recovery/Host arbitration, due-deadline handling and redraw coalescing. The application consumes/reschedules business work; no business callback execution moves into the Host. Device-error progress is not render animation and remains governed by R3.

## Dependencies and Coordination

- **Blocked by:** None.
- **Blocks:** T0004's integrated E2 verdict until deadline behavior is delivered.
- **Coordination risks:** T0003 changes non-frame fault reporting and possibly idle outcome shapes. Coordinate presenter/Host and application wait changes so neither future deadlines nor fatal state is discarded.

## Acceptance

- [ ] A non-drawable Host with a future host_deadline yields WaitUntil for that demand; absent a future demand it remains idle. (R2/E2)
- [ ] Absent/expired deadlines, including repeated expired input, cause no past WaitUntil, redraw/busy loop or animation Poll. The caller still owns due work. (R2/E2)
- [ ] Pending Runtime/recovery work does not force acquisition/submission or repeated recovery redraws while suspended; restoring drawability preserves normal coalesced rendering. (R2/E2)
- [ ] Drawable Runtime/recovery/Host arbitration, independent deadline storage and cross-window earliest-wait behavior remain correct. (R2/R6/E2)
- [ ] Native event-loop evidence checks future-deadline wake while rendering is suspended, or that specific evidence is explicitly left unverified; a scheduler return-value assertion is not represented as a native wake result. (E2; verification boundaries)
- [ ] Affected tests/callers/docs are migrated with evidence tied to the actual tree; no notification-policy or Host ownership change rides with the fix. (R6)

## Out of Scope

Moving business timers into widgets, using redraw to emulate wake delivery, automatic Store wakeup and GPU error-policy implementation.
