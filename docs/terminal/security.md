# Security and resource boundaries

[Terminal core](README.md) · [Embedding](embedding.md) · [Compatibility](compatibility.md)

Treat every output byte as untrusted, including output from local files, build tools, SSH sessions and WSL guests. Parsing safely does not make the producer trusted. The core may validate metadata or request a host effect; only the host can authorize and perform that effect. This is a terminal-processing boundary, not a sandbox for child processes or a guarantee of bounded total process memory/CPU.

## Host authority versus terminal state

| Output or interaction | Core behavior | Host obligation |
| --- | --- | --- |
| Title OSC `0/1/2` | Validated title/reset event | Treat as untrusted display metadata; do not make it an identity/security indicator or execute it. |
| OSC `7` | Validated working-directory URI metadata | Do not chdir, read files, access the network or map a remote hostname to local authority automatically. |
| OSC `8` | Bounded cell-linked URI attribute; pointer interaction can request activation | Require the intended user action and current session/target; validate scheme/path policy at execution. Parsing, hover and rendering must not fetch/open targets. |
| OSC `52` | Typed, bounded clipboard write candidate | Authorize the originating live session and current foreground context, then use an explicit host clipboard effect. |
| OSC `133` | Prompt/command lifecycle markers and optional exit-code metadata | Do not execute commands, inject input or treat the markers as authenticated shell identity. |
| Read-model `ActionTarget` | Neutral navigation/URI/file data | Validate content plus session/query lifetime, then independently authorize any action. |
| Paste/input | Mode-aware input encoding | Obtain clipboard text/user consent at the host boundary and apply paste policy before delivery. Bracketed paste is framing, not a shell-execution sandbox. |

The terminal's safe hyperlink activation path accepts http/https/mailto/file schemes; that allowlist alone does not make a URI safe. A `file:` link can refer to a remote or sensitive location, and a displayed label need not equal its target. Use structured OS-opening APIs, not string interpolation into a shell command. Custom hosts may choose a stricter policy or disable activation entirely.

SGR conceal hides presentation, **not** retained text, selection copy or search. Never use it to redact secrets. Snapshots, read pages, URI metadata and host logs can contain sensitive source data even when the visible glyphs are hidden.

## OSC 52: delivery is not authorization

The supported subset accepts selection `c` or empty selection, standard base64 with valid optional padding, and decoded UTF-8 without NUL. Tabs/newlines are preserved. Empty decoded text requests clearing the clipboard under the same authorization as a nonempty write. Reads (`?`), other selections, malformed/incomplete data and over-limit requests do not disclose or modify the clipboard.

`ClipboardDelivery` controls **pending retention**, not permission:

| Delivery | Behavior |
| --- | --- |
| `Latest` (engine default) | Replace the unexecuted candidate with the newest valid write; do not accumulate a write archive. |
| `First` | Keep the first pending candidate until drained; useful for immutable confirmation contents. |
| `Discard` | Reject new candidates and release an already pending write. |

A pure headless consumer should set `Discard` before feeding bytes if clipboard candidates are unnecessary. A live host must set policy before attaching/ingesting output; attachment preserves the configured delivery mode. Discard avoids decoded candidate allocation but does **not** eliminate bounded parser retention of OSC 52 wire bytes.

Harbor's application policy maps allow/deny/confirm to Latest/Discard/First. Its allow setting is **not** permission for every terminal process: admission requires the originating active live session, a non-minimized window and actual main-window foreground ownership. Execution revalidates eligibility and identity. Confirm retains one immutable request; its own foreground confirmation window can continue only that admitted request, not admit arbitrary background requests. Tab/source changes, closure or external focus loss cancel authority. Approval is single-use, not a permanent grant.

A custom embedder does not inherit this application policy merely by linking the engine. Preserve session identity when draining events; do not attach an old candidate to whichever tab is now active. Recheck immediately before the OS effect, release obsolete payloads, and report real clipboard-backend failure without a fabricated success or payload-bearing diagnostic. Closure also requires dropping host-owned candidates: the engine cannot revoke a `ClipboardWrite` already copied out by the host.

Owners and focused tests: [typed delivery/event API](../../crates/harbor-terminal/src/types.rs), [strict OSC 52 decoder](../../crates/harbor-terminal/src/parser/osc52.rs), [contract tests](../../crates/harbor-terminal/tests/osc52_contract.rs), [attachment policy tests](../../crates/harbor-terminal/src/clipboard_attachment_tests.rs), [application authorization/confirmation](../../src/clipboard.rs), [source-bound tab routing](../../crates/harbor-app/src/tab_manager.rs). These are implementation/test-source references, not new native clipboard test results.

## Resource limits and what they do not bound

These are current implementation constants, not user-configurable security settings or a universal memory budget. Consult the linked source before changing integration assumptions.

| Resource | Current bound / behavior | Owner |
| --- | --- | --- |
| CSI/DCS numeric accumulation | 16 parameters, 8 subparameters per parameter, 2 intermediate bytes; numeric cap 65535. Excess/overflow is handled by the parser's ignore/recovery rules, not an invitation to execute an accepted prefix as the whole request. | [Parser parameters](../../crates/harbor-parser/src/params.rs) |
| Ordinary OSC | 4096 retained bytes, excluding the special OSC 52 write allowance below. Overflow suppresses dispatch while framing/cancellation scanning continues. | [Parser strings](../../crates/harbor-parser/src/core.rs) |
| DCS/APC/PM/SOS payload processing | 4096-byte string cap before overflow/discard behavior; bounded callbacks are not image decoding support. | [Parser strings](../../crates/harbor-parser/src/core.rs) |
| OSC 52 wire data | 5,592,408 base64 data bytes only for exact `52;c;` / `52;;` prefixes, plus those prefix bytes; ordinary OSC retains its smaller cap. | [OSC-specific parser limit](../../crates/harbor-parser/src/core.rs) |
| OSC 52 decoded candidate | 4,194,304 UTF-8 bytes, no NUL; complete rejection rather than truncation. One pending candidate per engine delivery policy. | [Decoder](../../crates/harbor-terminal/src/parser/osc52.rs), [ClipboardWrite](../../crates/harbor-terminal/src/types.rs) |
| Title | At most 256 Unicode scalar values, control-free UTF-8; invalid/overlong requests rejected rather than truncated. | [Title parser](../../crates/harbor-terminal/src/parser/osc_title.rs) |
| OSC 7 | Host at most 255 bytes; decoded path at most 2048 bytes, with URI/UTF-8 validation. | [CWD parser](../../crates/harbor-terminal/src/parser/osc7.rs) |
| OSC 8 | URI at most 2048 bytes, optional ID at most 250 bytes; control-free UTF-8. Registry cleanup is reachability-based every 256 new allocations, not a fixed 256-live-link cap. | [Link parser](../../crates/harbor-terminal/src/parser/osc8.rs), [registry](../../crates/harbor-terminal/src/screen/hyperlink.rs) |
| Terminal reply buffer | 1024 bytes; complete replies accepted/dropped atomically, not truncated wire fragments. | [Screen](../../crates/harbor-terminal/src/screen.rs) |
| XTGETTCAP collector/reply | 256 request payload bytes, 256 framed reply bytes; overflow produces the defined failure response. | [Capability collector](../../crates/harbor-terminal/src/parser/xtgettcap.rs) |
| PTY reader queue | 32 events; reader uses a 4096-byte buffer and backpressures when the queue fills. Queue plus current chunk is not a bound on OS pipes, parsed state or host copies. | [Session I/O](../../crates/harbor-terminal/src/io.rs) |
| Default scrollback | 1000 history physical rows plus visible rows; not a byte budget. | [NormalBuf](../../crates/harbor-terminal/src/normal_buf.rs) |
| Bounded content page | Default 32 rows / 1024 units / 64 KiB payload; maxima 128 rows / 4096 units / 256 KiB. Counts source and copied URI/ID payload, not allocator overhead. Invalid budgets and indivisible oversized units return errors. | [Read model](../../crates/harbor-terminal/src/read_model.rs) |

A fixed parser accumulator bounds **unfinished sequence retention**, not total traffic or total work. Scanning still costs time proportional to incoming bytes, including discarded bytes. A reader drain can process a refilling queue; the queue capacity alone is not a wall-clock bound on one UI turn. Synchronous PTY writes, resize barriers and host effect calls also require scheduling/error policy.

Important remaining limits:

- The number of cells grows with requested grid dimensions, and resize prepares detached state. ConPTY's signed-16-bit cell conversion is not a safe allocation budget; impose practical host maxima before constructing/resizing.
- `Cell::suffix` extends retained combining/selector/ZWJ source; the cell writer has no fixed per-unit byte cap. A bounded read can reject a huge unit without retroactively bounding its storage in the screen. Row-limited history therefore does not prove byte-limited memory.
- Ordinary title/CWD/shell-marker events use a vector. The single-pending-write rule applies to clipboard candidates, not an all-events global cap. Drain or discard other events regularly; bound your own queued copies and delayed work.
- Full snapshots copy visible cells and their suffixes. Repeatedly storing snapshots/pages/search jobs can defeat per-operation bounds. Release obsolete data and limit concurrent sessions, jobs, queues and result archives.
- Unsupported image/file-transfer protocols have no supported decoder allocation contract. Their discard behavior is not proof that adding a decoder would be safe.
- GPU resource checks do not replace host device/surface recovery or an application-wide memory limit. Child processes themselves are not sandboxed by parser limits.

See [bounded read evidence](../terminal-read-model.md) and [literal-search bounds/coverage](../terminal-literal-search.md) for consumer-specific guarantees. A partial/cancelled/stale scan must not become a complete negative result or an actionable stale target.

## Recovery and diagnostics

Malformed/over-limit input should recover at supported framing/cancellation boundaries without executing host effects from rejected requests. Do not truncate a rejected URI or clipboard candidate into a different accepted value. Preserve parser state across valid arbitrary chunk boundaries and resize; do not strip escapes to manufacture a clean stream.

Log content-free reasons, lengths and safe identifiers where needed, not clipboard text, base64, terminal source, sensitive paths or arbitrary control-rich payloads. A rejected query/unsupported protocol should not trigger an invented OS action or disclosure reply. Rate-limit host diagnostics so a noisy producer cannot create an unbounded log/notification stream.

Existing source-level safety evidence includes [parser property tests](../../crates/harbor-parser/src/core/property_tests.rs), [OSC 52 boundary tests](../../crates/harbor-parser/src/core/osc52_tests.rs), [parser fuzz target](../../fuzz/fuzz_targets/parser.rs), [session streaming/EOF tests](../../crates/harbor-terminal/src/io.rs) and [read-model limit tests](../../crates/harbor-terminal/src/read_model/tests.rs). Test/fuzz source presence is not a claim of a newly executed campaign, global denial-of-service resistance or complete native-host acceptance. The [checklist security section](../protocol/checklist.md#36-protocol-security-limits) and [validation policy](../validation.md) retain the granular evidence and open gates.
