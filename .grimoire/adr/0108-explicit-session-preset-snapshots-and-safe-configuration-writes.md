# Explicit Session Preset Snapshots and Safe Configuration Writes

**Status:** Proposed
**Date:** 2026-11-02
**Sources:** Issue #157 refinement conversation; the owner approved saving Harbor-recorded launch program/arguments and the directory at save initiation, environment exclusion by default with explicit selection of configured overrides, and conflict-safe writes to the selected configuration file.
**Extends:** [ADR 0106](./0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0107](./0107-shared-palette-profile-flow-and-in-window-confirmation.md), [ADR 0062](./0062-watched-reload-and-new-session-launch-settings.md), and [ADR 0057](./0057-windows-installation-and-portable-isolation.md) for explicit user-requested writes only. Automatic configuration creation and portable/home isolation remain unchanged.

## Context

Quick saving introduces an explicit application-owned configuration write. A running shell can change directories, environment and foreground programs without exposing reliable launch-recipe information to Harbor. Capturing arbitrary runtime state could accidentally persist credentials or mistake a nested program for the original session launch.

Configuration can also be edited externally. Saving an older in-memory document over the user's current file would lose unrelated changes and comments.

## Decision

A save request captures Harbor's recorded program and arguments used to launch the source session, not the latest profile definition, typed command history, foreground command or a guessed running-shell recipe. Capture the directory when saving begins: use a usable local OSC 7 directory, otherwise the source session's initial directory. The later confirmation refers to that captured session state, not whichever tab or directory becomes current afterward.

Environment overrides are excluded by default. The user may explicitly select existing configured overrides for inclusion and confirm that they contain no passwords or tokens. Do not scrape the inherited system environment or a running shell's environment. This is not a credential store or automatic secret-persistence mechanism; environment values must not leak into logs or diagnostics.

Only the final explicit save confirmation authorizes writing a new named preset into the configuration file already selected for this process. Preserve unrelated settings and comments. Duplicate names do not overwrite existing presets. An externally changed file, invalid file content, or failed write must produce visible feedback rather than force an overwrite; retain the user's input for correction or retry.

Portable saves target only the selected executable-adjacent configuration, never the home configuration. This explicit operation does not authorize startup, watching or reload to create or overwrite settings automatically, nor fallback to another file on permission failure.

## Consequences

- Launch provenance and initial-directory information must remain available per session even after configuration reloads.
- A save draft has a stable source snapshot; tab switches and later OSC 7 updates cannot silently change the recipe being confirmed.
- Configuration persistence must protect both existing presets and unrelated external edits, with honest failure feedback.
- Explicit configuration writing does not bypass existing full-candidate validation or coherent accepted-generation publication.
