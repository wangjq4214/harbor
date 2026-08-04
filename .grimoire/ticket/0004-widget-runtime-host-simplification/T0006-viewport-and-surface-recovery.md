# Viewport and Surface Recovery

**Ticket ID:** T0006
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Done

## Goal

The main window preserves correct layout and rendering across resize, DPI changes, zero size, and every supported recoverable surface outcome.

## Layers

- [ ] **Runtime Host:** Forward resize/scale events and apply only fatal outcomes or resource-lifetime operations that cannot be handled by the borrowed integration.
- [ ] **Winit Runtime Integration:** Update frame-target configuration and apply success, suboptimal, lost, outdated, timeout, occluded, validation, zero-size, and fatal outcome policy.
- [ ] **Core Widget Runtime:** Update physical/logical viewport state before layout, hit testing, scene update, and encode, and invalidate only required work.
- [ ] **Terminal / Application Components:** Recompute terminal grid/GPU dimensions from the CustomPaint allocation and current scale without App-owned terminal resize orchestration.
- [ ] **Verification:** Test deterministic outcome classification and manually verify resize, minimize/restore, DPI transitions, and recovery redraw behavior.

## Approach

1. Move surface disposition policy out of App/terminal GPU helpers into the winit runtime integration, avoiding duplicate classifications.
2. Handle zero physical width or height by suspending acquisition and preserving enough state to resume on the next drawable resize.
3. On resize or scale change, update Surface configuration and Runtime viewport before layout and terminal allocation are observed.
4. Present successful frames; present then reconfigure suboptimal frames; reconfigure and request one retry for lost/outdated; skip timeout/occluded/validation; return unrecoverable failures to App.
5. Bound recovery retries so repeated failure waits for an external wake instead of spinning.
6. Remove App-owned viewport update, surface-recovery flags, and direct terminal GPU resize branches once integration tests cover the policy.

## Blocked by

- T0001 — defines frame and fatal outcome contracts.
- T0005 — establishes the main successful presentation path.

## Blocks

- T0007 — confirmation Runtime reuses this viewport and recovery policy.

## Acceptance

- [ ] Resize and scale-factor changes produce matching Widget layout, hit testing, terminal grid size, and rendered pixels.
- [ ] Zero-sized windows acquire and submit no frame and resume after a drawable resize.
- [ ] Success, suboptimal, lost, outdated, timeout, occluded, and validation outcomes follow the policy in spec 0004.
- [ ] Repeated recovery failure does not enter an unbounded redraw loop.
- [ ] Fatal GPU outcomes return to App for exit/error policy.
- [ ] `src/app.rs` no longer owns surface disposition, recovery-attempt state, viewport updates, or direct terminal resize orchestration.

## Out of Scope

- Recreating Device or Queue after device loss.
- Supporting another presentation backend.
- General animation or layout performance work.
- Confirmation-specific business behavior.
