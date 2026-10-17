# Targets, Boundaries and Capability Map

Harbor has three related products: a Windows-first terminal application, a reusable desktop widget toolkit, and an embeddable terminal engine. They need different learning paths and acceptance criteria. This document translates the comparison targets into review dimensions; it does **not** approve every upstream feature or replace existing requirements.

- **Application: WezTerm** informs productive terminal workflows, discoverable configuration, commands and operational documentation.
- **Widget: React and Flutter** inform complementary concerns: React's component/state mental models and Flutter's constraints, composable controls, rendering and desktop integration.
- **Terminal core: Ghostty** informs terminal semantics, VT reference quality, embedding boundaries, capability honesty and measured efficiency.

These are design/documentation benchmarks, not API, configuration, protocol-identity or feature-parity promises. No particular upstream release is certified. When starting an interoperability task, record the upstream specification/version, application, transport and expected bytes or behavior.

## Sources of Truth

| Question | Authority |
| --- | --- |
| What exists now? | [Current Status](current-status.md), linked public source and focused tests |
| What is the detailed VT scope? | [Protocol Checklist](protocol/checklist.md), not a copied checklist here |
| What is approved next? | [Next-Stage Product Plan](next-stage-plan.md) and applicable requirements |
| What is the execution order? | [Roadmap](roadmap.md) |
| What evidence establishes acceptance? | [Validation](validation.md) and revision-scoped records |
| Why were boundaries chosen? | Existing [Grimoire](../.grimoire/README.md) contracts and decisions |

The following matrices summarize capability families, not independent completion trackers. **Present** means an implementation exists for the stated subset; it does not mean every runtime gate passed. **Planned** refers to existing plans. **Decision required** identifies a comparison dimension without making it an approved feature.

## Application Compared with WezTerm

Audience: people using Harbor and people integrating it into their desktop workflows. Start at the [application handbook](app/README.md).

| Capability family | Harbor baseline and remaining boundary | Existing plan / reference |
| --- | --- | --- |
| Installation and operation | Present: Windows x64 MSI/portable construction and distinct configuration paths. Packages are unsigned; installer lifecycle and downloaded-release acceptance remain separately scoped. | [Distribution status](current-status.md#windows-distribution), [release gates](roadmap.md#release-gates) |
| Terminal sessions | Present: shell launch, tabs, independent session state and hidden output processing. Split workspace is planned; hiding is not closing. | N07; [system ownership](architecture/system.md) |
| Configuration | Present: TOML, startup fallback, strict manual/watched reload, live terminal appearance/bindings/fonts, new-session launch settings. No Lua compatibility. | N04; [configuration reference](app/configuration.md) |
| Commands and discovery | Present: stable command IDs, configured shortcuts and context-sensitive routing. Palette and profiles are planned, not hidden current options. | N05/N06; [command reference](app/commands.md) |
| Search and review | Present: bounded current-session literal search and navigation, selection/copy and scrollback. Regex/case options, broader shell navigation and user history budgets remain follow-ups. | N08/N09/N15; [search contract](current-session-search.md) |
| Shell and transport integration | Present: OSC title/directory/link/prompt metadata subsets. Consuming metadata is not full command navigation or local-directory inheritance. WSL/SSH/tmux need workload-specific evidence. | N03/N09/N11; [compatibility](terminal/compatibility.md) |
| Appearance and accessibility | Present: Windows Acrylic/fallback, decorations and local search-panel glass mechanisms. Unified theme selection, accessibility bridge and complete high-contrast/reduced-effect policy are not implied. | N13/N14; [remaining toolkit work](widget/catalog.md#remaining-toolkit-work) |
| Workspace lifetime | Cross-window movement, restoration and persistent/reconnectable processes are deferred and require distinct ownership/security designs. Restoring a layout is not reviving a process. | N17 |
| Programmability and remote domains | CLI/IPC automation, user scripting/plugins and application-owned SSH/multiplexing domains are **decision required**, not silently approved by the WezTerm comparison. Running `ssh` in a shell is a different capability. | A separate requirement and threat model are needed before implementation. |

Application documentation should answer: how do I perform the task, what setting/command controls it, when does it take effect, and how do I recover when it fails? It should not require reading implementation tickets.

## Widget Compared with React and Flutter

Audience: Rust developers composing interfaces or embedding the toolkit, including non-terminal applications. Start at the [widget guide](widget/README.md).

| Dimension | Borrow from the comparison | Harbor baseline / boundary |
| --- | --- | --- |
| Declarative composition | React component decomposition; Flutter small composable widgets | `Component` builds disposable `View` descriptions; retained fibers own identity. `view!` is experimental Rust-expression syntax, not JSX/Dart compatibility. |
| State and identity | Explicit state ownership, stable keys, preservation/reset rules | UI-thread pull-based `Signal`, hook slots and host-owned Store/action queues. Do not assume React render snapshots, automatic action wakeups or Flutter lifecycle methods. See [concepts](widget/concepts.md). |
| Layout | Flutter parent constraints and separate measurement/painting | Parent-directed Flex, sizing/wrappers, committed layout observation and logical-pixel input geometry. Grid, wrap, baseline/intrinsic protocols and virtual collections are not all delivered. |
| Input and controls | Complete desktop interaction contracts, not merely visual widgets | Focus, capture, typed shortcuts/actions, buttons, scrolling and reusable single-line text editing exist. Menus, selection-control families and generic modal/anchored overlays remain planned. |
| Escape hatches | Explicit external/platform boundaries | `CustomPaint`, Portal and host effects exist; a root escape is not a modal dialog manager. Product commands, session IDs and glass recipes stay outside generic widgets. |
| Rendering and materials | Retained scene, clipping, composition and bounded resource work | Solid/gradient decoration, rounded clips and local blur/edge refraction exist. No general SVG/path/image/filter framework or arbitrary desktop sampling is promised. |
| Accessibility and environment | Flutter semantics, desktop keyboard access and localization discipline | Keyboard behavior is not UI Automation. Semantic tree/UIA, locale/direction propagation and richer environment contracts remain explicit work. |
| Developer experience | React Learn; Flutter catalog, recipes and widget tests | [Learn](widget/learn.md), [catalog](widget/catalog.md), [recipes](widget/recipes.md) and [testing](widget/testing.md) describe current APIs; the [catalog backlog](widget/catalog.md#remaining-toolkit-work) bounds future breadth. |

Concurrent React-style rendering, server rendering/hydration, DOM compatibility, mobile navigation and Flutter Material/Cupertino parity are not current commitments. Additional platforms require their own host and acceptance work.

## Terminal Core Compared with Ghostty

Audience: terminal embedders, VT application authors and maintainers. Start at the [terminal guide](terminal/README.md).

| Dimension | Harbor baseline / required discipline | Reference |
| --- | --- | --- |
| Layering | GPU-independent engine; session I/O adapter; optional concrete wgpu renderer; host-owned surfaces and authorization. Optional rendering is enabled by default in the terminal crate. | [Embedding](terminal/embedding.md), [ownership](architecture/terminal-session-ownership.md) |
| Text and buffers | Preserve source units, width/copy agreement, primary reflow versus alternate clipping and finite history. Do not equate UTF-8 bytes, scalars, logical units and columns. | [Concepts](terminal/concepts.md), [read model](terminal-read-model.md) |
| VT semantics | Exact defaults, mode/reset lifetime, replies, unsupported behavior, fragmented input and bounded strings. Safe framing/discard is not semantic support. | [VT reference](terminal/vt.md), [detailed checklist](protocol/checklist.md) |
| Embedding contract | Explicit output draining, event/reply handling, read/ack/replay, wakeups, resize barriers and teardown. The current renderer boundary is concrete and single-consumer, not a universal multi-renderer API. | [Embedding](terminal/embedding.md) |
| Compatibility | Advertise only implemented capabilities; distinguish Console clients, VT clients and ConPTY/remote transport effects. No Kitty identity or native Unix support inferred from a subset. | [Compatibility](terminal/compatibility.md) |
| Security | Treat terminal output as untrusted; parser events request host decisions rather than grant clipboard, launch or filesystem authority. Bound per-object and aggregate host retention. | [Security](terminal/security.md) |
| Performance | Measure parser/core, renderer and platform work separately, including hidden sessions, reflow and effects. Existing historical captures do not certify current budgets. | N15; [profiling](performance/profiling-guide.md), [optimization work](performance/profiling-guide.md#measured-optimization-backlog) |

Ghostty's application UI, language choices, platform coverage and public API shapes are not requirements for Harbor. Kitty graphics/keyboard and other optional protocols remain their own bounded work packages, with transport and security gates.

## Cross-Layer Acceptance

Every delivered capability should connect five things:

1. A reader-facing behavior or integration contract, including unsupported and failure behavior.
2. A named owner and explicit dependencies across application, widget, engine and platform boundaries.
3. Public source/API and focused tests or executable examples.
4. Revision/environment-scoped runtime or performance evidence where the behavior depends on a platform or application.
5. Updated authoritative status and navigation, without copying historical results into a new blanket pass.

Use the [system architecture](architecture/system.md) for the end-to-end paths and [documentation maintenance guide](README.md#maintenance) for authoring/check commands. Do not expand a release gate merely because an upstream project has more features.

## Upstream Reading

These links define comparison dimensions, not compatibility claims:

- [WezTerm features](https://wezterm.org/features.html), [configuration](https://wezterm.org/config/files.html), [CLI](https://wezterm.org/cli/cli/index.html), [multiplexing](https://wezterm.org/multiplexing.html)
- [React Learn](https://react.dev/learn), [Rules of React](https://react.dev/reference/rules)
- [Flutter widget catalog](https://docs.flutter.dev/ui/widgets), [architecture learning path](https://docs.flutter.dev/learn/pathway/how-flutter-works), [testing](https://docs.flutter.dev/cookbook/testing)
- [Ghostty Terminal API](https://ghostty.org/docs/vt), [VT sequence reference](https://ghostty.org/docs/vt/reference)
