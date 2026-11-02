# Confirmed Quick Save of the Current Session Recipe

**Ticket ID:** T0003
**Source:** [Spec 0052 R5–R7; E8–E14](../../spec/0052-named-session-profiles-and-quick-save.md); [ADRs 0107](../../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md) and [0108](../../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md).
**Status:** Todo

## Goal

A user names the current session's known launch recipe, reviews a stable snapshot in an internal overlay, and explicitly saves a new profile into the selected configuration without losing unrelated content or accidentally persisting runtime state.

Keep naming, confirmation, capture and persistence together: a Save control without the real safe commit outcome is not this ticket's delivery.

## Affected Surfaces

Application palette/editor presentation, new-session menu and commands, exclusive-popup/focus/input integration, Host-owned source capture/configuration writer, selected-path and accepted-generation publication. Consume T0001's source provenance/profile validation. Config serialization tests must preserve comments/unrelated settings.

## Direction and Boundaries

The naming surface is input-only (no commands/results) with a matching-height/material/corner Save… button on the right inside the same row/panel. Button and Enter only advance to confirmation. Use one current-window internal confirmation overlay, not a new winit window. Capture the source session when saving begins and keep that draft stable through review.

Environment is excluded by default; only deliberately selected existing configured overrides can be included after user confirmation that no passwords/tokens are present. Preserve confidentiality and do not scrape runtime/inherited environment.

Persist only on final confirmation, protecting current file content, comments and duplicate names; report conflict/error and retain input rather than force overwrite.

## Dependencies and Coordination

- **Blocked by:** T0001's profile schema/acceptance and recorded source launch/initial-directory facts.
- **Blocks:** T0004's integrated save/reopen/launch and native confirmation evidence.
- **Parallel with:** T0002 after T0001. This ticket can verify writer → accepted catalog without the completed chooser; T0004 verifies their combined visible flow.
- **Coordination:** Share palette/lifetime and Host routing changes with T0002. Preserve automatic config behavior, portable isolation, existing native paste confirmation and complete-candidate acceptance.

## Acceptance

- [ ] Both menu and palette expose the save flow. Input-only naming has the right-hand styled Save… button; Enter/click are equivalent and do not write.
- [ ] Empty/duplicate names receive nearby feedback, disable advance and cannot bypass validation through keyboard; later commit-time collisions also do not overwrite.
- [ ] Confirmation is internal to the existing window, displays captured name/program/arguments/directory and environment inclusion, avoids unredacted environment-value disclosure, and provides return-to-edit plus final Save.
- [ ] Returning retains the draft; cancel writes nothing. Repeated/held advance Enter cannot auto-confirm, IME candidate confirmation does not submit, and underlying panel/terminal input is gated.
- [ ] Capture uses recorded source launch program/arguments and save-time usable local OSC 7 directory, otherwise source initial directory. Later tab/OSC 7/profile changes do not silently alter the draft.
- [ ] Do not capture history, inferred foreground/nested commands, shell runtime changes or the whole system environment. Default saving includes no environment overrides; explicit selection includes only the elected configured entries, without credentials/value leakage.
- [ ] Only final confirmation adds a new preset to the selected config file; preserve unrelated settings/comments and existing presets. Installed/portable targeting follows the retained selected path, never fallback home/other path on write failure.
- [ ] External modification, invalid document, commit-time name collision and write/replacement faults preserve existing file content, give honest visible feedback and retain input. Protect against partial/truncated failed writes and late conflicts rather than relying only on naming-time checks.
- [ ] Successful commit is durable and enters the accepted catalog; watcher/manual interleaving cannot duplicate entries, publish stale state or partially accept live fields. Distinguish incomplete disk/in-memory outcomes from success.
- [ ] Late editor/clipboard/IME/action delivery cannot modify or save a replacement draft; close/cancel restores eligible terminal focus with no PTY leakage.
- [ ] Focused flow-to-writer-to-catalog tests cover the observable contract using synthetic data and failure injection. Supply Windows keyboard/IME/no-new-native-window outcomes or honest omissions to T0004.

## Out of Scope

Full graphical profile editing, overwriting existing presets, credentials, runtime environment/process cloning, native save-confirmation windows, automatic config writes, configuration-path migration and unrelated clipboard-confirmation changes.
