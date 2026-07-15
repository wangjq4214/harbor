# Declarative GPU UI with host-owned native dialogs

Harbor will rename `harbor-render` to `harbor-gpu` and place declarative widget layout, interaction, and reconciliation in `harbor-ui`. UI runtime instances share one GPU context and font atlas while owning their window surface; the application shell owns windows, terminal sessions, PTY effects, and application-modal dialog lifecycle. This keeps GPU rendering reusable, lets `Terminal` participate as a `CustomPaint`-backed UI component, and retains native dialog windows without coupling generic UI components to host I/O.
