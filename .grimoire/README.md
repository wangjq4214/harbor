# Grimoire

Current project knowledge and durable architectural records live here. Execution tickets and generated plans are temporary and should be removed after their durable information and verification obligations are preserved.

## Navigation

- [Project terminology and domain relationships](./CONTEXT.md)
- [Terminal protocol terminology](./CONTEXT-terminal-protocol.md)
- [Architecture decisions](./adr/)
- [Requirements specifications](./spec/)

## Acceptance Baseline

The owner confirms that all existing specifications are implemented and accepted. Specs remain regression contracts, not an active backlog. Earlier pending-verification statements in retained execution evidence are historical; owner acceptance does not assert that previously unrun tests were executed or that recorded discrepancies were independently resolved.

## Lifecycle

- `CONTEXT*.md` records current project terminology.
- `adr/` preserves architectural decisions and their supersession history.
- `spec/` holds ongoing behavior and acceptance contracts, including explicit unresolved discrepancies.
- `ticket/` is temporary execution planning. A ticket is removed only after implementation or explicit disposal is established, durable information survives elsewhere, references are repaired, and the exact content is recoverable or deletion is explicitly authorized.
- Generated HTML plans and run logs are not durable knowledge and are not retained after their work is complete.
