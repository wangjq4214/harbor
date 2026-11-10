# Two-Level Profile Chooser Verification

## Outcome: automated and ConPTY suite PASS; desktop IME and window focus tracked under T0004

This document records automated and Windows ConPTY execution evidence for ticket T0002 under spec 0052. It establishes that the shared command-palette two-level profile chooser, tab-rail entry, query filtering, return navigation, and safe FIFO action admission are verified.

## Scope and execution environment

- Platform: Windows 10/11 x64, Microsoft ConPTY runtime 1.24.260710001.
- Target surfaces: `crates/harbor-app/src/palette.rs`, `crates/harbor-app/src/tab_view_ui.rs`, `src/shell.rs`, `src/tab_coordinator.rs`.
- Target contracts: Spec 0052 requirements R4 and relevant R1–R3; acceptance criteria E1, E6, E7.

## Verified contracts and observations

### 1. Dual entry points and shared catalog (R4, E1)

- Selecting **New session from profile…** (`app.open-profile`) from the command palette or **New from profile…** from the tab rail opens the second-level chooser in the same palette panel.
- The chooser displays accepted named profiles with their display names and stable IDs.
- Ordinary New Tab (Ctrl+T) remains completely separate and continues launching the default `[shell]`.
- Verified by:
  - `crates/harbor-app/src/profile_chooser_tests.rs`: `chooser_opens_from_both_entry_points_and_shares_catalog`
  - `crates/harbor-app/src/tab_view_ui.rs`: rail compact and expanded entry composition.

### 2. Search filtering and navigation within the panel (R4, E7)

- The filter input performs case-insensitive substring matching on profile names and identifiers.
- Up/Down keys navigate visible matches; Enter, the checkmark button, or row click selects a profile.
- Return control (left arrow button, Escape key, or mouse Back) navigates back to the primary command palette without launching.
- Empty or no-match results disable confirmation and prevent phantom launches or leaking Enter keys to the underlying terminal.
- Verified by:
  - `crates/harbor-app/src/profile_chooser_tests.rs`: keyboard navigation, return handling, empty filter behavior.

### 3. Action admission and stale selection protection (R4, E6, E7)

- Profile requests carry a generation-scoped profile launch intent.
- Reloading configuration while the chooser is open updates available choices coherently.
- Stale, removed, or edited selections in the action queue are rejected at admission time and will not launch an obsolete or substitute recipe.
- One-shot execution: duplicate or held input executes at most once.
- Terminal focus is restored upon launch or cancellation; no consumed input reaches the PTY.
- Verified by:
  - `src/action_batch_tests.rs`: `profile_fifo_admits_one_real_transaction_and_targets_the_new_terminal_or_preserves_old_on_failure`
  - `src/action_batch_tests.rs`: `profile_fifo_revalidates_edits_removal_source_gate_and_replacement`
  - `src/session_profile_tests.rs`: `chooser_fifo_to_real_child_and_accepted_reload_catalog`.

## Scoped exclusions and deferred items

- Production desktop IME candidate selection and physical window focus transitions under desktop multi-window scenarios remain tracked in integrated T0004 verification.
- Quick Save preset creation is covered under T0003 and T0004.
