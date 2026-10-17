# Harbor Documentation

Choose the product you are using or building. Current usage, future scope and acceptance evidence are separate: a documented implementation is not a claim that every native/runtime or release gate passed.

## Reading Guide

| Audience | Start here | Continue with |
| --- | --- | --- |
| Harbor user | [Application handbook](app/README.md) | [Getting started](app/getting-started.md), [configuration](app/configuration.md), [commands](app/commands.md), [troubleshooting](app/troubleshooting.md), [releases](app/releases.md) |
| UI developer | [Widget guide](widget/README.md) | [Learn](widget/learn.md), [concepts](widget/concepts.md), [catalog](widget/catalog.md), [recipes](widget/recipes.md), [testing](widget/testing.md) |
| Terminal embedder / VT author | [Terminal guide](terminal/README.md) | [Concepts](terminal/concepts.md), [embedding](terminal/embedding.md), [VT reference](terminal/vt.md), [compatibility](terminal/compatibility.md), [security](terminal/security.md) |
| Maintainer / contributor | [System architecture](architecture/system.md) | [Targets and boundaries](targets.md), [Current Status](current-status.md), [Roadmap](roadmap.md), [Validation](validation.md), [documentation maintenance](#maintenance) |

The application comparison is **WezTerm**; widget concepts and catalog draw from **React and Flutter**; terminal-core contracts draw from **Ghostty**. [Targets](targets.md) defines the dimensions and non-goals. These comparisons are not full feature/API compatibility promises.

### Task-Oriented Shortcuts

- **Run from source:** [project README](../README.md#build-and-run).
- **Find a setting/default or understand reload:** [configuration reference](app/configuration.md).
- **Rebind a command:** [command grammar and inventory](app/commands.md).
- **Search terminal history:** [user workflow](app/getting-started.md), then [integration contract](current-session-search.md).
- **Build a field or understand IME/clipboard:** [text input](widget-input.md), [verification boundaries](widget-input-verification.md).
- **Understand constraints, Flex and overflow:** [layout contract](flex-layout.md).
- **Embed without a PTY/GPU:** [terminal embedding](terminal/embedding.md) and the [crate quick start](../crates/harbor-terminal/README.md).
- **Check an escape sequence:** [VT reference](terminal/vt.md), then the [detailed protocol checklist](protocol/checklist.md).
- **Find current limitations:** [Current Status](current-status.md), not the presence of an item in a plan.
- **Understand next work:** [Roadmap](roadmap.md) and [N01–N17 product plan](next-stage-plan.md).

## Technical References

| Area | Document |
| --- | --- |
| Workspace-library installation, features and quick starts | [`harbor-widget` README](../crates/harbor-widget/README.md), [`harbor-terminal` README](../crates/harbor-terminal/README.md) |
| Full-system ownership, event/output/frame flow and failure paths | [System Architecture](architecture/system.md) |
| View/Fiber/runtime, host integration and retained rendering | [Widget Runtime Architecture](architecture/widget-runtime.md) |
| Remaining desktop toolkit capabilities | [Widget catalog and backlog](widget/catalog.md#remaining-toolkit-work) |
| Parent-directed constraints and measurement | [Flex Layout](flex-layout.md) |
| Reusable single-line controller/editor/field | [Widget Text Input](widget-input.md), [verification](widget-input-verification.md) |
| Engine/session/renderer ownership and teardown | [Terminal Session Ownership](architecture/terminal-session-ownership.md) |
| Bounded history reads, provenance and stale targets | [Terminal Read Model](terminal-read-model.md) |
| Bounded literal matching and coverage | [Terminal Literal Search](terminal-literal-search.md) |
| Current-session application search and historical integration evidence | [Current-Session Search](current-session-search.md) |
| Modern decorations, palette and mouse evidence | [SGR](modern-sgr-acceptance.md), [palette](osc-palette-acceptance.md), [mouse](multi-encoding-mouse-acceptance.md), [mouse probe](mouse-probe.md) |
| Box drawing and metric/resource lifecycle | [Procedural box drawing](procedural-box-drawing-evidence.md), [decoration metrics](decoration-metric-lifecycle-evidence.md) |
| Performance procedures, historical captures and measured backlog | [Profiling Guide](performance/profiling-guide.md) |
| Development helper commands and probe safety | [Scripts](../scripts/README.md) |
| Skill-maintained terminology, decisions and requirements | [Grimoire](../.grimoire/README.md) (read-only during documentation maintenance) |

Generate the workspace API reference locally with:

```bash
cargo doc --locked --no-deps -p harbor-widget --features harbor-widget/winit -p harbor-terminal
```

Open the resulting `target/doc/harbor_widget/index.html` or `target/doc/harbor_terminal/index.html`. These paths are generated artifacts, not checked-in documentation links. The library interfaces are pre-1.0; source/rustdoc is the exact API surface, while these guides explain behavior and integration.

## Document Responsibilities

- **Application / Widget / Terminal guides** teach current behavior, tasks and integration. Link detailed contracts rather than duplicating acceptance logs.
- **Targets** defines comparison dimensions and boundaries, not new approved work packages or another completion checklist.
- **Current Status** owns the source-backed product inventory and links to focused evidence.
- **Roadmap** owns delivery order; **Next-Stage Product Plan** owns N01–N17 scope and acceptance outcomes. A planned feature is not a current API.
- **Protocol Checklist** owns detailed VT coverage. Checked rows require concrete implementation plus focused tests or reproducible evidence for their stated scope.
- **Validation** owns shared quality gates and evidence requirements; a prescribed command is not an executed pass.
- **Architecture and focused references** explain current ownership and invariants. The widget catalog separates current APIs from remaining toolkit work.
- **Performance records** separate historical measurements, procedures and pending work.
- **Grimoire** remains a skill-maintained knowledge base. Public documentation changes do not rewrite its records, statuses, numbering or supersession history.

## Status and Evidence Rules

Use two independent axes:

- **Implemented / partial / not implemented** describe the stated behavior in source.
- **Needs runtime evidence** means platform/application acceptance remains unproven.
- **Planned / investigate / deferred** describe future work, not code coverage.
- **Configured automation** means a harness/workflow exists, not that a remote run passed.

When records disagree, inspect source, focused tests and revision-scoped runs. Repair the current reader-facing summary; report unresolved contract conflicts instead of rewriting skill-maintained history. A successful rerun does not prove an intermittent failure fixed, and protocol inventory counts are not release-readiness percentages.

## Maintenance

Keep public documentation in English and reachable from the appropriate application/widget/terminal landing. The tables above define content ownership: document each fact once and link it from summaries. Check APIs/settings/commands against current source; label planned behavior and unsupported paths instead of inventing runnable interfaces. `.grimoire/` is skill-maintained and stays read-only during this work.

Preserve unique contracts, unresolved discrepancies and revision-scoped evidence when consolidating pages. Repair incoming file/fragment links before deleting an obsolete page; an old record is not useless merely because it predates the current UI. Avoid public links requiring private reports or disposable `target/` artifacts. Do not bulk-delete local evidence or untracked files.

Run [Validation's standard checks](validation.md#standard-quality-gates), inspect the diff and review factual/visual behavior manually. The Markdown checker covers tracked and nonignored untracked files, including crate guides and common local heading fragments. It is not a full CommonMark renderer: reference/HTML links, complex nested markup, external websites and factual claims still require review. A source export without Git uses a warned fallback; Git failures in a checkout fail.

### Rust Examples and API Reference

- `rust`: complete deterministic examples safe to execute without a native window, PTY or clipboard; Rustdoc hidden setup uses `# ` lines.
- `rust,no_run`: complete examples that compile but require interactive/native resources. Compilation is not runtime acceptance.
- `text`: conceptual pseudocode. An ignored example needs an explicit reason and is not verified by a passing run.
- State required features and public dependencies. Keep secrets/terminal/clipboard payloads out of logs; preserve consent gates on destructive probes.

Run `python scripts/docs/check_examples.py --include-readmes` to verify public widget/terminal guides and both crate READMEs. The helper creates an owned package under `target/docs-examples`, seeds its derived lock from the workspace pins and rejects external dependency drift; product manifests and the root lockfile remain unchanged. It does not execute non-Rust fences or native `no_run` examples. `--prepare-only` generates without checking; `--help` describes selection.

The API-reference command and generated paths are listed above. For all changes, keep delivery distinct from acceptance, retain relevant revision/environment/exclusions, and do not infer platform parity or a release pass from a narrow automated result.
