# Windows-Version-Agnostic Backdrop Backend

**Status:** Completed
**Date:** 2026-09-01

## Context

ADR 0026 defines a three-tier backdrop chain whose tier selection depends on Windows version, Windows App SDK initialization, and `DesktopAcrylicController::IsSupported()`. Placing that branching directly in the window bootstrap would leak platform details into the host. Alternatives were inline platform branching in `try_resume`, a dedicated crate, or a host-layer trait with a selector.

## Decision

Introduce a host-layer `WindowBackdropBackend` trait with three real Windows implementations (WASDK acrylic, AccentPolicy acrylic, opaque fallback) plus a non-Windows no-op backend. A `select_backend()` selector encapsulates OS-version detection, WASDK bootstrap, and `IsSupported()` checks, returning `Box<dyn WindowBackdropBackend>`. The host holds only the boxed backend and calls `apply(window, &WindowBackdropStyle) -> BackdropStatus { tier, backdrop_available }`. The module lives in `src/app/window_backdrop.rs`, not a separate crate, because only the host consumes it.

## Consequences

- `try_resume` contains no Windows version or tier branching; tier selection is unit-testable behind the selector.
- The existing `backdrop_available` data flow (terminal clear policy, root fallback fill) is fed by `BackdropStatus`.
- Future macOS/Linux translucency fits as additional backend implementations without host changes.
- The backend is selected once per window at bootstrap; tier degradation follows ADR 0026's fallback chain.
