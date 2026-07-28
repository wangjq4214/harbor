# PTY I/O and Input Events

**Ticket ID:** T0004
**Source:** [Spec: 0002-terminal-render-consolidation](../spec/0002-terminal-render-consolidation.md)
**Status:** Todo

## Goal

`Terminal` integrates PTY I/O: accepts `impl Read + Send + 'static` and `impl Write + Send + 'static`, spawns an internal reader thread to feed bytes into the parser. Input event path is fully wired: `CustomPaint` → queued external input → App forwards → `Terminal::handle_event()` → writes to PTY.

## Layers

- [ ] **Parser:** None — complete
- [ ] **Terminal:**
  - `Terminal::new()` accepts PTY handles
  - Internal `spawn_reader_thread()` — `std::thread::spawn` blocking read, loop: `read()` → channel send → mark dirty → wake redraw
  - `Terminal::write_pty(&mut self, bytes: &[u8])` — calls `pty_write.write_all(bytes)`
  - `Terminal::handle_event(&mut self, event: &UiEvent)` — translates keyboard events to VT sequences, calls `write_pty`
  - `Terminal::snapshot(&self) -> TerminalSnapshot` — for App to query latest state
- [ ] **Widget:** None — T0002 complete
- [ ] **App:**
  - `src/app.rs` — pass PTY handles when creating Terminal
  - Event loop: `runtime.dispatch(event)` → `drain_external_input()` → `terminal.handle_event(&event)`
  - Remove old `TerminalWorkerClient` channel communication (or keep inert; T0005 cleans up)
  - `render_frame()` calls `terminal.snapshot()` instead of worker-provided snapshot

## Approach

### 4.1 PTY reader thread

1. `Terminal::new()` signature update:
   ```rust
   pub fn new(
       size: TerminalSize,
       pty_read: impl std::io::Read + Send + 'static,
       pty_write: impl std::io::Write + Send + 'static,
       gpu: &GpuContext,
       font_book: FontBook,
       metrics: TextMetrics,
   ) -> Self;
   ```
2. Use `std::sync::mpsc::channel` — reader thread sends `Vec<u8>`, main thread drains before `render()`:
   - **Rationale:** avoids Mutex contention on the hot path; compatible with winit single-threaded model.
3. `spawn_reader_thread()` — `thread::spawn(move || { loop { let n = pty_read.read(&mut buf)?; tx.send(buf[..n].to_vec()); } })`
4. `Terminal` holds `rx: Receiver<Vec<u8>>`. Before each `render()`, call `drain_pty()` to consume and `put_bytes()` all buffered chunks.

### 4.2 PTY writes

5. `Terminal::write_pty(&mut self, bytes: &[u8])` — direct `self.pty_write.write_all(bytes)`.
6. Synchronous write — caller is on main thread; brief blocking is acceptable.

### 4.3 Input events

7. `Terminal::handle_event(&mut self, event: &UiEvent)` — match keyboard events, use `InputEncoder::request()` to produce VT sequence, call `self.write_pty(&encoded)`.
8. `Terminal` holds `InputModes` state (import from `harbor_types` or inline).

### 4.4 App adaptation

9. `src/app.rs` — `resumed()`: create PTY then pass directly to `Terminal::new()`.
10. `window_event()` — `runtime.dispatch(event)` → `for (id, event) in runtime.drain_external_input() { terminal.handle_event(&event); }`
11. `render_frame()` — `terminal.drain_pty()` → `runtime.encode(...)` (Terminal handler auto-looked up)
12. Old `terminal_worker` / `TerminalWorkerClient` — keep inert (T0005 deletes).

## Blocked by

- T0003 — requires Terminal struct owning Screen and render components

## Blocks

- T0005 — cleanup removes old worker communication code

## Acceptance

- [ ] `Terminal::new()` accepts `impl Read + Write`, spawns reader thread
- [ ] PTY output automatically appears in rendering (launch shell, see prompt)
- [ ] Keystrokes reach PTY (type, shell responds)
- [ ] Terminal resize causes correct PTY output layout
- [ ] Reader thread exits cleanly on `Terminal` drop (channel disconnects, thread exits loop)
- [ ] External `terminal_worker` no longer required to push updates to rendering

## Out of Scope

- PTY write buffering / `BufWriter` optimization (synchronous writes suffice)
- `bytes` crate introduction (`Vec<u8>` is sufficient)
- Paste confirmation dialog (separate future ticket)
- `harbor-pty` crate deletion or refactor (PTY creation logic may remain; Terminal just no longer communicates through worker channels)
