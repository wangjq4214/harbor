# Harbor Documentation

Start with the question you want to answer. Current implementation, future plans, and verification evidence are deliberately separate.

## Reading Guide

| Question                                          | Read                                                                                                   |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| What is Harbor, and how do I run it?              | [Project README](../README.md)                                                                         |
| What actually exists, and what is still partial?  | [Current Status](current-status.md)                                                                    |
| What should we work on next?                      | [Roadmap](roadmap.md)                                                                                  |
| What is included in the next-stage plan?          | [Next-Stage Product Plan](next-stage-plan.md)                                                          |
| Is a particular VT behavior supported?            | [Protocol Checklist](protocol/checklist.md)                                                            |
| What must be tested before calling work complete? | [Validation](validation.md)                                                                            |
| How do I configure the current release?           | [Startup configuration](../README.md#startup-configuration) and [example TOML](../config.example.toml) |

## Technical References

| Area                                                    | Document                                                      |
| ------------------------------------------------------- | ------------------------------------------------------------- |
| `harbor-widget` installation and usage                  | [`harbor-widget` README](../crates/harbor-widget/README.md)   |
| `harbor-terminal` parsing, PTY, input and rendering     | [`harbor-terminal` README](../crates/harbor-terminal/README.md) |
| Widget/runtime ownership and host integration           | [Widget Runtime Architecture](architecture/widget-runtime.md) |
| Desktop widget foundations and remaining capabilities   | [Widget Capability Plan](widget-capability-plan.md)           |
| Parent-directed flex measurement and allocation         | [Flex Layout](flex-layout.md)                                 |
| Historical memory evidence                              | [Memory Baseline](performance/memory-baseline.md)             |
| Reproducible performance capture procedure              | [Profiling Guide](performance/profiling-guide.md)             |
| Remaining measured optimization work                    | [Optimization Plan](performance/optimization-plan.md)         |
| Terminology, architectural decisions, specs and tickets | [Grimoire](../.grimoire/README.md)                            |

## Document Responsibilities

- **Current Status** is the source-backed product inventory. It distinguishes implementation scope from missing runtime evidence and links to lower-level facts.
- **Roadmap** owns execution order, dependencies and release scope. It does not declare a feature implemented merely because it has a milestone.
- **Next-Stage Product Plan** owns the agreed N01–N17 work-package scope, constraints, follow-up increments and acceptance outcomes. It is not a second completion checklist.
- **Protocol Checklist** owns detailed VT coverage. A checked row requires a clear implementation plus focused tests or reproducible runtime evidence. An open row may mean missing, partial, or not sufficiently verified; its note should say which.
- **Validation** owns shared quality gates, runtime matrices and evidence-record requirements. A prescribed command is not a recorded successful run.
- **Architecture documents** describe current boundaries and invariants. **Widget Capability Plan** distinguishes present infrastructure from optional widgets and product integration.
- **Performance documents** separate historical captures, repeatable procedures and still-open optimization work. Do not rewrite an old measurement to look current.
- **Grimoire artifacts** preserve terminology, decisions and implementation contracts. Follow their lifecycle rules; do not delete or rewrite historical decisions just because the roadmap changes.

## Status and Evidence Rules

Use implementation and validation as separate axes:

- **Implemented / partial / not implemented** describe the stated behavior in source.
- **Needs runtime evidence** means platform or application acceptance is still unproven, even if model tests exist.
- **Planned / investigate / deferred** describe future work, not code coverage.
- **Configured automation** means a workflow or harness exists, not that its latest run passed.

When documents disagree, inspect source, focused tests and recorded runs first. Keep applicable architectural decisions explicit, including supersession; a future plan does not override the current implementation. Correct the canonical fact and link summaries to it instead of maintaining contradictory duplicate checklists.

## Maintenance

- Keep project documentation in English.
- Keep volatile counts out of overviews; use `python scripts/checklist_summary.py` for protocol counts. Those counts are not a release-readiness percentage.
- Update source-backed status and focused coverage when behavior changes; update roadmap state only when work actually starts or its gate is met.
- Preserve existing paths/anchors where practical, and repair references when moving or restructuring content.
- Run `python scripts/check_docs.py` for language and local-file links. It does not validate Markdown fragments or factual claims; inspect those separately.
- Run both documentation scripts after a documentation-only change. Code/runtime tests are additionally required when implementation or validation claims change.
