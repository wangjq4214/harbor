# Unified declarative Widget runtime

Harbor UI uses a single generic `Widget<A>` lifecycle with statically composed children, retained runtime state, and explicit child-intent mapping to a typed host intent. Dialog remains a native-window Widget whose host owns only window lifecycle, while Terminal privately composes every terminal visual layer and emits semantic intents for the application shell to apply; this replaces renderer-layer and shell-overlay protocols so there is one layout, event, and paint model without per-node type-erasure allocations.

## Considered options

- Dynamic `Box<dyn Widget<A>>` trees were rejected because static composition avoids per-node allocation and dynamic dispatch.
- A global UI-intent enum was rejected because it would couple `harbor-ui` to application commands.
- Moving terminal session state and external effects into widgets was rejected because the application shell must retain PTY, clipboard, and terminal-session ownership.
