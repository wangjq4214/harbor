# Widget Text Input

`harbor-widget` exposes `TextEditingController`, `EditableText` and `TextField`.
The controller owns editing state, the undecorated editor owns input behavior,
and the field composes the editor with padding and a `BoxDecoration`.
The application search entry uses this same field; query limits and search
commands remain application-owned.

```rust
use harbor_widget::{TextEditingController, TextField};

let controller = TextEditingController::new("");
let field = TextField::new(controller.clone())
    .placeholder("Search")
    .on_change(|text| {
        // Submit application intent here; do not run search while painting.
        let _ = text;
    });
```

Keep the controller outside repeated builds, or retain it in a component hook.
Do not create a fresh controller on every rebuild if editing should persist.
The field is already focusable: use `.focus_handle(handle)` to request its
focus through the runtime/host instead of wrapping it in another focus stop.

## Editing Contract

- Public selection and composition positions use UTF-8 byte offsets. The
  controller clamps invalid positions backwards to extended grapheme boundaries.
  Caret movement/deletion keeps combining sequences, flags and ZWJ emoji intact.
- `set_value` updates related state together; `set_text` places the caret at the
  end. Both return false without mutation when text is rejected. Invalid initial
  text passed to a constructor panics. Control characters and Unicode line/
  paragraph separators are rejected rather than converted to submission.
- `with_acceptance(initial, predicate)` validates resulting committed text after
  selection replacement. Applications may impose their own byte budgets.
- `on_change` reports user committed-text changes only. Programmatic writes,
  selection movement and IME preedit do not invoke it. `on_submit` receives
  Enter modifiers; `on_escape` permits consumer-owned cancellation behavior.
- Windows editing chords are Ctrl+A/C/X/V/Z/Y and Ctrl+Shift+Z for redo. Arrows,
  Home/End, Shift extension, Backspace/Delete and pointer drag edit/select locally.
  Unrelated application shortcuts retain their routing. Consumed editor chords
  do not invoke terminal copy/paste or send PTY input.
- Each accepted replacement is one undo operation, with no time-based grouping.
  History retains up to 100 operations, not a fixed byte budget. Programmatic
  value replacement clears history. Transient IME updates are not history steps.
- Copy/cut use only selected source text. Cut deletes only after a successful
  clipboard write. Tagged paste/cut results are ignored after focus loss,
  controller/revision replacement, removal or a relevant mode change.
- Read-only fields permit selection/copy but not user mutation; disabled fields
  accept no editing and are excluded from focus traversal.

The shared `VerticalDirection::{Up, Down}` is passed to
`on_vertical_navigation` on both editor layers. It observes unmodified vertical
navigation only, after IME handling; it adds no horizontal navigation. Up/Down
correspond to the former false/true values. Callbacks retain UI-local captures,
committed-text data/order, and Enter modifiers.

## Presentation and Hosting

`TextField` accepts `.placeholder(...)`, `.padding(...)`, `.decoration(...)`,
`.foreground(...)`, `.disabled(...)` and `.read_only(...)`. `EditableText` exposes the
same editing callbacks without the outer border/padding. `padding` returns
`Result` and accepts finite nonnegative values, including zero; `foreground`
returns `Result` and accepts finite RGBA components (including outside [0,1]).
Handle an intermediate Result before chaining another setter. Foreground affects
edited text only, not decoration, caret or selection. Editor content is
clipped; horizontal scrolling keeps the active caret usable. Caret and selection
are separate draw primitives, not characters in the text.

With initialized native text resources, editing positions use the widget
renderer font's raster advances at the current DPI. Headless runtimes without
those resources use the runtime's synthetic fallback advance; they do not prove
native typography or IME candidate positioning.

`WinitWindowHost` executes tagged `ClipboardEffect::Editor` requests and returns
results via `Runtime::complete_clipboard`. A custom host must honor that same
request/result contract (read returns text, successful write returns an empty
string, errors are content-free). Passing a result to ordinary `dispatch` is
not a substitute for targeted completion. Legacy application clipboard effects
and terminal paste/OSC authorization remain separate.

See the [testing guide](widget/testing.md) for component tests and troubleshooting.
