# Developer Guide

Internal development notes for contributors to Harbor. For using Harbor or embedding its libraries, start with the [public documentation](../README.md).

## Understand the implementation

- [System architecture](architecture.md): modules, ownership and end-to-end flows.
- [Widget runtime](widget-runtime.md): retained identity, state, layout, input and frame encoding.
- [Terminal sessions](terminal-session.md): engine/session/renderer ownership, resize and shutdown.
- [Performance profiling](profiling.md): repeatable measurement methods, not historical results.

## Track the work

- [Progress](progress.md): implemented scope, known gaps and outstanding validation.
- [Roadmap](roadmap.md): delivery order and the retained N01–N17 work-package identifiers.
- [Grimoire](../../.grimoire/README.md): domain terminology, architectural decisions and detailed requirements.

Keep these pages concise and current. Do not store test reports, screenshots, run logs or copied acceptance tables here. Run `python scripts/docs/check_docs.py` for documentation checks; the separate Rust example compiler also remains available. The checklist-statistics script has been removed.
