# GPU runtime boundary

`harbor-gpu` owns only the domain-neutral wgpu runtime: instance, adapter, surfaces, device, queue, and reusable GPU primitives. `harbor-ui` owns terminal rendering and consumes that runtime; the application shell owns `TerminalInteraction`, including selection, clipboard, PTY, pointer, scrolling, and timing state. This deliberately avoids both terminal/PTY dependencies in the GPU runtime and host I/O dependencies in the UI renderer while allowing the shell to supply renderer overlays through a narrow interface.
