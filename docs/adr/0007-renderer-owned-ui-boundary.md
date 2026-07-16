# Renderer-owned UI boundary

`harbor-render` sits between `harbor-ui` and `harbor-gpu`: `harbor-ui → harbor-render → harbor-gpu`. `harbor-ui` owns Widget configuration, layout, event routing, typed intents, and Terminal visual projection; it has no GPU API or resource dependency. `harbor-render` owns the shared GPU runtime, per-window Render targets, frame execution, fonts, glyph atlases, pipelines, and renderer caches. `harbor-gpu` owns only backend initialization, device/queue access, and low-level surface creation/configuration; it contains no UI rendering primitive.

Widgets emit ordered, bounds-clipped generic paint commands through renderer-owned `PaintContext`; `CustomPaint` remains a UI Widget. Terminal is therefore an ordinary CustomPaint node: UI reads the GPU-agnostic Terminal model and emits generic rect/text/glyph commands, while renderer remains independent of `harbor-terminal` and exposes no terminal-specific rendering API. Render targets own surface acquisition, black clear, submit, present, per-target Render environment, and GPU cache lifetime; hosts own windows, events, redraw scheduling, and clear terminal dirt only after `FrameOutcome::Presented`.

## Considered options

- Keeping raw `wgpu` access in UI was rejected because it leaks GPU lifetimes and prevents renderer replacement or containment.
- A terminal-specific renderer API, or a renderer dependency on `harbor-terminal`, was rejected because it makes terminal a special render path rather than a normal UI node.
- Having the application construct or retain `GpuRuntime` was rejected because it preserves a path for application-owned GPU policy.
- Returning UI-built draw lists was rejected in favor of the ordered command `PaintContext`, which avoids an additional command-object lifecycle while preserving declarative Widget traversal.
