# Shared Palette Profile Flow and In-Window Confirmation

**Status:** Proposed
**Date:** 2026-11-02
**Sources:** Issue #157 refinement conversation: the owner requested reusing the command palette with a second-level profile selector, an input-only palette presentation for saving, and a confirmation overlay inside the current window rather than a new winit window.
**Related:** [ADR 0106](./0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0077](./0077-extensible-exclusive-application-popups.md).

## Context

Named profile selection and quick saving need application UI entry flows. The owner selected shared command-palette interaction rather than a separate profile editor, and explicitly rejected a separate native confirmation window for this save operation.

## Decision

Reuse the command palette for opening named session profiles, with profile choice presented at a second level.

For quick saving, present the palette with an input field and no command list. Pressing Enter advances to a confirmation surface; it does not itself complete saving. The confirmation is an internal overlay in the existing application window, not a newly created winit window.

These are profile-flow choices. They do not change the independent native paste-confirmation window policy or authorize a general suspended-popup stack contrary to ADR 0077.

## Consequences

- Profile opening and quick saving share the palette presentation and editing interaction.
- The save flow requires no additional native window lifecycle.
- Application-specific profile and persistence effects remain subject to the existing application-command/Host ownership boundary.
