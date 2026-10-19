# Three-Second Notifications Including Errors

**Status:** Testing
**Date:** 2026-10-19
**Sources:** The user's notification refinement changed the requested timeout from five to three seconds, followed by explicit confirmation: “Override it; notifications must not remain permanently!” when asked whether this includes error notifications and overrides their persistent presentation.
**Supersedes:** [ADR 0064](./0064-widget-and-application-component-ownership.md), only its persistent-actionable-error constraint in decision 3. Also replaces the no-auto-expiry/persistent-error requirements in [spec 0025](../spec/0025-portal-and-configuration-notifications.md), R3 and E10; the remaining contract is unchanged.
**Related:** [ADR 0063](./0063-root-composable-escape-and-quiet-configuration-notices.md).

## Context

The existing notification contract keeps actionable errors visible until manually dismissed. During notification appearance refinement, the user explicitly rejected permanent notifications, including errors, and selected three seconds rather than the earlier five-second suggestion.

## Decision

Harbor's existing main-window floating notifications automatically close after three seconds of display, including actionable errors and unrecovered-failure notices. Error severity is not an exception permitting permanent presentation. Manual early dismissal remains available.

This is application notification visibility policy, not a change to configuration acceptance, recovery, or failure truth. Automatic closure does not mean a failure was repaired or a rejected configuration was accepted. Keep application ownership of eligibility, duplicate suppression and dismissal history, and preserve non-modal top-right presentation without focus theft.

## Consequences

- The old persistent-error constraint must not prevent the three-second timeout. The superseded clauses of spec 0025 are historical, not the current lifetime acceptance criteria.
- This changes Harbor's product policy, not timeout defaults for every consumer of the reusable widget.
- Implemented in application-owned notice state and the main-window idle-turn deadline arbitration; repeated identical errors do not extend the timer or resurrect after expiry. Focused automated checks pass (168 app tests, 462 widget unit tests, four component-boundary tests, seven configuration-notice integration tests, all-target/all-feature clippy and formatting checks). A bounded Windows smoke of the built binary showed shared glass over real terminal pixels, a centered close icon and automatic disappearance of both an ignored-setting warning and an invalid-TOML error without UI input. Exact three-second boundaries are deterministic headless evidence; native screenshots are sampled observations, not precise timer measurements. Native hover/press, cross-monitor DPI and performance acceptance remain unverified.
