# Add Automated Repository Layout Gate

**Ticket ID:** T0006
**Source:** [Spec 0058](../../spec/0058-standardized-repository-file-layout.md), [ADR 0112](../../adr/0112-standardized-repository-file-layout.md)
**Status:** Todo

## Goal

Enforce Harbor's standardized repository layout automatically via `scripts/ci/check_layout.py`, unit tests, `just layout-check`, and the CI `quick` job so layout drift fails immediately.

## Affected Surfaces

- `scripts/ci/check_layout.py` (new)
- `scripts/tests/test_check_layout.py` (new)
- `scripts/README.md` (index `check_layout.py` under `ci/`)
- `justfile` (add `layout-check` recipe)
- `.github/workflows/windows-quality-gates.yml` (invoke `python scripts/ci/check_layout.py` in `quick` job)

## Approach

- Implement `scripts/ci/check_layout.py` using Python's standard library, enumerating Git-tracked and non-ignored working-tree files (matching the repository enumeration pattern in `scripts/docs/check_docs.py`).
- Validate all four structural invariants:
  1. Tracked top-level directories match the explicit allowlist (`.github`, `.grimoire`, `assets`, `crates`, `docs`, `fuzz`, `installer`, `scripts`, `src`, `tests`, `third_party`).
  2. Zero `mod.rs` files under `src/` or `crates/*/src/`.
  3. Zero `*_tests.rs` files directly at `src/` or `crates/*/src/`.
  4. `docs/` and `scripts/` contain only `README.md` and directories at their immediate root.
- Write unit tests in `scripts/tests/test_check_layout.py` covering a clean tree and each individual rule violation.
- Add `layout-check` to `justfile` and run `python scripts/ci/check_layout.py` in the `quick` job of `.github/workflows/windows-quality-gates.yml`.

## Dependencies and Coordination

- **Blocked by:** T0001, T0002, T0003, T0004, T0005 (all structural cleanups must be in place so the gate passes on the repository).
- **Blocks:** None.
- **Coordination risks:** Keep `check_layout.py` pure stdlib Python 3.12-compatible and cross-platform so it works in both Windows CI and local development.

## Acceptance

- [ ] `python scripts/ci/check_layout.py` exits `0` on the repository.
- [ ] `python -B -m unittest discover -s scripts/tests -v` passes all tests including `test_check_layout.py`.
- [ ] `just layout-check` runs `scripts/ci/check_layout.py` and succeeds.
- [ ] `.github/workflows/windows-quality-gates.yml` runs `python scripts/ci/check_layout.py` in the `quick` job.

## Out of Scope

- Enforcing formatting or Rust lint rules already covered by `cargo fmt` and `cargo clippy`.
