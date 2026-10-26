# Development Progress

[Developer guide](README.md) · [Roadmap](roadmap.md)

The general inventory below retains the `e6d140c` documentation baseline. The command-palette entry is updated for integrated revision `2efb762`; its bounded Windows evidence does not revalidate unrelated runtime, performance or release claims.

## Implemented scope

| Area | Delivered | Current boundary / entry point |
| --- | --- | --- |
| Terminal content | Incremental VT parsing, scrollback, primary/alternate buffers, content anchors, transactional resize/reflow | [Engine](../../crates/harbor-terminal/src/lib.rs); styled ConPTY live-tail clipping is intentional, not lossless archival. |
| Text and rendering | Combining/VS/ZWJ source retention, DirectWrite fallback, bounded color-emoji presentation, procedural box drawing and modern decorations | [Text backend](../../crates/harbor-text/src/backend/dwrite.rs), [renderer](../../crates/harbor-terminal/src/render/); complex-script shaping and ligatures are not delivered. |
| Terminal extensions | Modern underline/color, conceal/overline, OSC 4/104 palette, multiple mouse encodings, focus and synchronized output, title/CWD/link/shell-marker metadata, host-authorized OSC 52 writes | [VT reference](../terminal/vt.md); metadata parsing is not a shell-workflow UI. OSC 52 reads and Kitty keyboard/graphics are not implemented. |
| Windows sessions and tabs | Bundled ConPTY, application-owned tab/session identity, selection/copy, paste confirmation, IME and session shutdown | [Tab manager](../../crates/harbor-app/src/tab_manager.rs), [session ownership](terminal-session.md); split panes and native Unix sessions are not delivered. |
| Configuration | Startup recovery, manual and default-on watched reload, live colors/bindings/terminal fonts, new-session shell/environment/directory settings | [Configuration guide](../app/configuration.md), [watcher](../../src/configuration_watch.rs); reload is distinct from debug widget-library HMR. |
| Search | Bounded read model and literal matcher, current-session highlights/navigation, query/session cancellation and stale-target gates | [Read model](../../crates/harbor-terminal/src/read_model.rs), [matcher](../../crates/harbor-terminal/src/literal_search.rs), [application integration](../../crates/harbor-app/src/search.rs). Literal, case-sensitive, at most 128 UTF-8 query bytes and 1024 matches; no regex or cross-session search. |
| Command palette | Existing visible tab/reload/copy/paste/scroll/search commands through the shared registry and Host path | [User guide](../app/commands.md#command-palette); [outstanding native acceptance work](../../.grimoire/spec/0030-command-palette.md#delivery-and-outstanding-acceptance). Profiles/panes/appearance commands remain deferred under #156. |
| Widget toolkit | Declarative runtime, flex layout, focus/actions, scrolling, view macro, Portal/notifications and reusable single-line editing | [Widget guide](../widget/README.md); this is not a complete accessible desktop toolkit or rich-text framework. |
| Appearance | Acrylic/fallback, rounded decoration, gradients, local backdrop blur and edge refraction used in the search panel | [Workspace](../../crates/harbor-app/src/workspace.rs), [backdrop filter](../../crates/harbor-widget/src/widgets/backdrop_filter.rs); full-product themes/glass and measured visual/performance acceptance remain separate. |
| Windows distribution | MSI and portable ZIP, executable-adjacent portable settings, shared package/quality workflows and separate release publication | [Release guide](../app/releases.md), [packaging](../../scripts/packaging/), [workflow](../../.github/workflows/windows-quality-gates.yml); configured automation is not proof of a successful published/downloaded release. Packages are unsigned. |
| Documentation | Public application/widget/terminal guides, with explanatory architecture, progress and roadmap separated into `docs/developer/` | Historical reports/screenshots remain deleted. Markdown checking and Rust example compilation are retained; only checklist-summary automation is removed. |

## What advanced since the older plan

- Configuration reload is no longer a future implementation: manual reload, file watching, launch settings and integrated configuration seams exist. Remaining work is the outstanding integrated human/runtime coverage.
- Search is no longer just a proposed consumer: the bounded engine APIs, application commands, current-geometry highlights and editable search panel are integrated.
- Reusable text editing and local glass/gradient/refraction primitives exist. They do not imply complete desktop controls, whole-product appearance work or universal native acceptance.
- Packaging and release workflows exist. Ordinary-user installation, lifecycle and live download/publication checks must not be described as missing implementation or assumed complete.

## Outstanding correctness and validation

| Area | Remaining work | Contract |
| --- | --- | --- |
| Unicode/reflow | Resolve the documented autowrap-disabled final-column selector-width discrepancy; complete integrated resize, clipboard, alternate-buffer and large-history cost coverage. | [Unicode](../../.grimoire/spec/0015-unicode-terminal-text-correctness.md#unresolved-acceptance-and-discrepancy), [resize](../../.grimoire/spec/0014-content-preserving-resize-reflow.md) |
| Native terminal/resource lifecycle | Finish applicable Windows input, mouse, emoji, decoration/font replacement, cross-monitor DPI and create/hide/close/failure scenarios. Investigate the intermittent font-replacement GPU comparison rather than treating a successful rerun as a repair. | [Core/renderer boundary](../../.grimoire/spec/0016-terminal-engine-wgpu-renderer-boundary.md), [decoration lifecycle](../../.grimoire/spec/0022-box-drawing-and-decoration-lifecycle.md) |
| Configuration and editor | Complete integrated IME, physical editing/held-key, selection/copy, font/DPI, terminal coexistence and supported HMR checks; earlier slice confirmations are not blanket package acceptance. | [Reload](../../.grimoire/spec/0024-watched-reload-and-launch-settings.md#delivery-and-outstanding-acceptance), [text input](../../.grimoire/spec/0028-reusable-single-line-text-input.md) |
| Search and appearance | Close the retained integrated output/resize/lifecycle matrix, measure large-history work and memory, and check local material appearance under live content and DPI changes. | [Search](../../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md#outstanding-integrated-acceptance), [search material](../../.grimoire/spec/0029-search-panel-glass-appearance.md) |
| Distribution/release | Finish genuine ordinary-user/UAC install, applicable upgrade/uninstall/scope/portable scenarios and separately authorized live-tag/download smoke; retain parser-safety and representative Windows dogfood coverage. | [Windows packages](../../.grimoire/spec/0023-windows-msi-and-portable-release.md) |
| Performance/transport | Obtain comparable current-tree allocation/latency measurements; investigate code-page and Kitty transport behavior before making compatibility claims. | [Profiling](profiling.md), [roadmap](roadmap.md) |

## Not implemented or deferred

Named profiles, split panes, complete shell-integration workflows, Kitty keyboard/graphics, cross-session/regex search, cross-window movement and session persistence remain future work. Native Unix PTY spawning is a [stub](../../crates/harbor-pty/src/unix.rs), distinct from WSL/SSH inside Windows.

Update this page when delivered behavior or a known gap changes. Keep execution logs and test-result tables out of the progress summary; implementation and acceptance are separate states.
