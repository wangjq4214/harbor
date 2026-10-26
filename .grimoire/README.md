# Grimoire

Current project knowledge and durable architectural records live here. Execution tickets and generated plans are temporary and should be removed after their durable information and verification obligations are preserved.

## Navigation

- [Project terminology and domain relationships](./CONTEXT.md)
- [Terminal protocol terminology](./CONTEXT-terminal-protocol.md)
- [Architecture decisions](./adr/)
- [Requirements specifications](./spec/)

## Acceptance Baseline

The owner confirms that all existing specifications are implemented and accepted. Specs remain regression contracts, not an active backlog. Earlier pending-verification statements in retained execution evidence are historical; owner acceptance does not assert that previously unrun tests were executed or that recorded discrepancies were independently resolved.

## Widget API Contract Index

The standalone UI/API review is retired. Its F/U identifiers remain historical traceability labels, not an active backlog or evidence of current defects. The following ADRs/specs retain the selected contracts; naming mappings and preserved runtime/application boundaries no longer depend on the review's section numbers. Use the acceptance baseline above and each contract's evidence limitations rather than the retired review's implementation snapshot.

| Historical finding / unit | Durable contract |
| --- | --- |
| F1 — Deferred roots and content-slot lifetime | [ADR 0079](./adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md), [spec 0034](./spec/0034-deferred-view-root-and-content-slot-boundary.md) |
| F2 / U1 — Child cardinality and replacement | [ADR 0080](./adr/0080-unify-child-cardinality-and-replacement-semantics.md), [spec 0035](./spec/0035-unified-child-cardinality-and-attachment.md) |
| F3 — Control naming and UI-local callbacks | [ADR 0081](./adr/0081-preserve-click-naming-and-standardize-disabled-builders.md), [ADR 0082](./adr/0082-ui-thread-local-widget-callbacks.md), [spec 0036](./spec/0036-ui-thread-callbacks-and-control-api-consistency.md) |
| F4 — Composable buttons and content measurement | [ADR 0083](./adr/0083-composable-buttons-and-shared-control-surfaces.md), [spec 0037](./spec/0037-composable-buttons-and-content-measurement.md) |
| F5 — Explicit layout and fallible view nodes | [ADR 0084](./adr/0084-explicit-layout-trees-and-local-configuration.md), [spec 0038](./spec/0038-explicit-layout-components-and-fallible-view-nodes.md) |
| F11/F12 / U2 — Event consumption and unconditional commit | [ADR 0085](./adr/0085-return-driven-event-consumption-and-unconditional-commit.md), [spec 0039](./spec/0039-return-driven-event-consumption-and-commit.md) |
| F10/F16 / U3 — KeyChord normalization and private Runtime control | [ADR 0086](./adr/0086-normalized-key-chords-and-private-runtime-control.md); the former spec 0040 contract remains there and in public compile/mounted regressions |
| F6–F8 / U4 — Naming, configuration and validation | [ADR 0087](./adr/0087-widget-api-naming-and-validation-boundaries.md), [spec 0041](./spec/0041-widget-api-naming-and-configuration-validation.md) |
| F9/F13/F15 / U5 — Host roots, deadlines and shared GPU faults | [ADR 0088](./adr/0088-native-host-root-deadline-and-shared-gpu-fault-contracts.md), [spec 0042](./spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) |
| F14 / U6 — Public renderer paint order | [ADR 0089](./adr/0089-public-renderer-paint-order-boundary.md), [spec 0043](./spec/0043-public-renderer-paint-order.md) |

## Lifecycle

- `CONTEXT*.md` records current project terminology.
- `adr/` preserves architectural decisions and their supersession history.
- `spec/` holds ongoing behavior and acceptance contracts, including explicit unresolved discrepancies.
- `ticket/` is temporary execution planning. A ticket is removed only after implementation or explicit disposal is established, durable information survives elsewhere, references are repaired, and the exact content is recoverable or deletion is explicitly authorized.
- Generated HTML plans and run logs are not durable knowledge and are not retained after their work is complete.
