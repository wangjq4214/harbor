# Terminal concepts

[Terminal core](README.md) · [Embedding](embedding.md) · [VT reference](vt.md)

A terminal is not a text editor buffer, a log string or a texture. It retains a cell grid and history, interprets control sequences incrementally, and exposes different projections for painting and content consumers. The distinctions below matter when output, resize and user interaction happen between reads.

## From bytes to text units to cells

The [streaming parser](../../crates/harbor-parser/src/core.rs) accepts arbitrarily fragmented input. A UTF-8 character or escape sequence may cross calls; an input chunk is not a line, frame or protocol transaction. `Terminal::put_bytes` preserves that parser state between calls. `process_output` uses the same parsing but also snaps normal-buffer review to the bottom unless suppressed.

The [cell model](../../crates/harbor-terminal/src/model.rs) stores a leading scalar in `Cell::ch` and original trailing scalars in `Cell::suffix`. Combining marks, variation selectors and supported ZWJ continuations can extend one retained unit. A unit is not necessarily one Unicode scalar, byte or occupied cell, nor a promise of complete Unicode grapheme segmentation or complex-script shaping.

**Oversized-input safety exception:** live primary and alternate admission allow at most **1024 UTF-8 source bytes per unit**, including the leading scalar ([ADR 0075](../../.grimoire/adr/0075-calibrated-history-and-text-unit-limits.md)). Before the first extension that would exceed this cap, admission freezes the scalar-aligned prefix; further extensions of that unit are discarded, even if a smaller scalar would fit. The next independent unit resumes normal admission. Fragmenting PTY reads cannot bypass the cap. Copy, read/search, rendering and reflow use only that surviving prefix; dropped text is not recoverable or replaced by synthetic/orphan source. This deliberate exceptional-input loss is separate from font fallback and does not fix the known no-wrap final-column selector-width discrepancy. See [Spec 0031 result summary](../../.grimoire/spec/0031-bounded-history-and-performance-budgets.md#delivery-and-closure-summary).

| Representation | Meaning |
| --- | --- |
| UTF-8 bytes | Transport/source encoding; byte positions are not terminal columns. |
| Retained text unit | Admitted lead-plus-suffix source text with model-assigned width. Content APIs never split it to satisfy a page budget. |
| Lead cell | Carries source text, width, colors, attributes, protection and optional hyperlink ID. |
| Wide continuation | The second occupied cell of a width-two unit: width zero, `wide_continuation = true`; not a second copied/searchable character. |
| Presentation cue | An isolated combining mark may need a dotted circle for display. The synthetic cue is not source text. |

Use `Cell::raw_text()` for a cell's retained text, not just `ch`. Do not concatenate wide continuation cells as if they were source spaces. The [cell writer](../../crates/harbor-terminal/src/screen/edit/cell_writer.rs) owns width assignment and unit extension; font rasterization must fit the assigned grid footprint rather than changing it. A missing glyph or monochrome fallback does not authorize changing copied text.

SGR conceal is presentation only. Source text, width and background remain; copy and content reads can still expose the original text. It is **not redaction**.

## Physical rows and logical lines

`NormalBuf` stores cells in a ring with per-row metadata; see [storage](../../crates/harbor-terminal/src/normal_buf.rs) and the [shared logical decoder](../../crates/harbor-terminal/src/logical_content.rs).

- A **physical row** has the current number of columns. A viewport row index is just a display coordinate for that read.
- A **logical line** joins physical rows connected by recorded soft wraps. An explicit hard boundary remains a boundary, including empty lines. Reaching a page edge does not create a newline.
- Metadata includes a never-reused buffer-local `LogicalLineId`, absolute cell/atom starts, meaningful extent, soft-wrap state and truncated-head evidence. Ring slots are reused; logical IDs are not derived from those slots.
- The retained sequence includes history plus live rows, oldest first. A reviewed viewport can show older rows while the child continues to write at the live cursor.
- The default normal-buffer history limit is 1000 physical rows, not 1000 logical lines or a byte budget. Eviction can remove only the prefix of a long logical line; remaining offsets must not be renumbered as though the lost text still existed.

For example, `abcdefgh` in a four-column grid can span two physical rows without a source newline between them. By contrast, `abcd\r\nefgh` contains a hard boundary. Copy/search should join only the soft-wrapped case. Trailing display capacity is not automatically meaningful source content.

### Blanks and retention

Blank provenance matters. Explicit/interior blanks and visibly styled blanks can be retained content; fresh capacity and generated padding before a wide unit are not equivalent source spaces. Underlines, overlines, background and inverse can make blanks visually meaningful. Hyperlink fallback and strike alone do not make an ordinary blank visibly meaningful under this model.

Primary reflow trims ordinary trailing spaces; it is not lossless text archival. For the live Windows ConPTY primary viewport, style-only overflow can be clipped on narrowing rather than pushed into new live rows. Widening does not resurrect that discarded tail. History/non-ConPTY styled retention and alternate rectangular retention differ. The bounded read API exposes the result, not a hidden archive.

## Primary and alternate buffers

The primary screen supports scrollback/review. Alternate-screen programs operate on isolated state rather than editing primary history. The [screen implementation](../../crates/harbor-terminal/src/screen.rs) saves/restores primary state and can park alternate state for later non-clearing entry.

- `?47` requests a non-clearing buffer switch; `?1047` and `?1049` request clearing entry; `?1048` saves/restores the cursor. Do not infer identical semantics for all four modes from one successful application session.
- Primary resize reflows retained logical content. Alternate resize preserves a rectangle, clips/fills it and has no primary-style scrollback reflow. Saved primary/parked alternate geometry must also be kept coherent.
- Selection and cursor anchors are reconciled through edits and transitions. Session-scoped state such as focus reporting, synchronized-output nesting and palette policy is not simply an application-owned copy per visible buffer.
- RIS/reset and clearing/replacement can establish new content identity. A return to numerically identical coordinates does not restore old read proofs.

Use `Terminal`, which handles parser buffer-switch requests and remaining bytes in order. The lower-level `TerminalParser::put_bytes` returns `consumed` and `alt_request`; ignoring either can leave bytes unparsed or apply them to the wrong buffer.

## Positions, anchors and validity

There are several coordinate systems. Do not interchange them:

| Position | Appropriate use | Not a guarantee of |
| --- | --- | --- |
| Display `(row, col)` | Paint/hit testing in one viewport | Stable content after scroll/resize |
| Physical row generation + column (`GenPos`) | Existing selection/projection machinery | A durable external content identity |
| Logical line + atom offset + affinity | Insertion boundaries and anchor reconciliation | Unchanged source text |
| `ContentRange` with row proofs/version | Validated content decoration/navigation | Host permission or current query ownership |

Internal [content anchors](../../crates/harbor-terminal/src/content_anchor.rs) track logical insertion boundaries with `Before`/`After` affinity and projection hints. They can survive an edit even when the text at that location changes. Public `read_model::LogicalPosition` reuses that logical scheme; an atom offset is not a byte offset or a column count.

For external consumers, [bounded reads](../../crates/harbor-terminal/src/read_model.rs) add stronger provenance:

- `ContentVersion` contains engine `SessionId`, `BufferKind`, content generation and semantic revision. An engine replacement gets a different session identity, not a reused tab index.
- Writes, suffix changes, erases, style/provenance and structural line changes advance semantic revisions. Row proofs conservatively invalidate ranges touching modified rows, even if the edit was outside the selected substring or later restored the same text.
- Viewport, cursor-only and palette-only changes, drawing, invalidation and renderer acknowledgement are not content revisions.
- Committed width/height resize/reflow, reset and replacement invalidate old generations. A no-op resize or discarded detached preparation does not.
- Strict continuation requires the same revision. Explicit `rebase_content_read` can establish a new consistency point at an unchanged checkpoint; it neither validates prior pages nor extends the captured scope end. Refresh/validate ranges separately before combining or acting on them.
- Only the active buffer kind/generation is eligible. An unchanged parked buffer can become eligible again on restoration; a replaced generation cannot.

Owned pages can outlive the terminal; **ownership is not validity**. Revalidate before publication and activation. The host must separately reject closed/replaced sessions and cancelled queries. Shell EOF alone is not host closure and need not invalidate retained text.

See `harbor_terminal::read_model` and `harbor_terminal::literal_search` in the generated API reference for continuation, validation and coverage rules.

## Three kinds of reads

| API | Copies/observes | Side effects / obligations |
| --- | --- | --- |
| `screen()` | Borrowed live screen | Read-only via `Terminal`; do not retain display coordinates as content identity. |
| `snapshot()` / `read_update(now)` | Owned visible cell projection; update adds damage, selection, preedit, appearance and scheduling | No PTY drain, event consumption or automatic acknowledgement. A full projection is not a full-history read. |
| `read_content(scope, budget)` | Bounded complete units, owned styles/links and logical boundaries from visible or retained scope | Independent content validity; does not acknowledge renderer damage or consume parser events. |

`drain_pty`, `drain_and_snapshot` and `frame_demand` are different: they can ingest queued output. `drain_output_events` consumes metadata/effect requests once; it is not a snapshot operation.

## Damage is a renderer contract, not a content revision

[Damage tracking](../../crates/harbor-terminal/src/damage.rs) records changed cells/rows. `DirtyRange` uses a row and half-open column interval. A [TerminalUpdate](../../crates/harbor-terminal/src/update.rs) is a coherent owned projection for **one** acknowledgement baseline:

1. Initially, and after `invalidate_update`, damage is `FullUpload`.
2. Otherwise, ranges accumulate relative to the last acknowledged state. Repeated reads and skipped draws do not clear them. A renderer can widen uploads for its own buffer/geometry requirements.
3. `acknowledge_update(&update)` compares projection epoch, current snapshot, selection, preedit, appearance and backdrop. It rejects stale state and leaves work pending. A successful acknowledgement clears damage/pending redraw and advances the epoch; reusing the old update is not another valid acknowledgement.
4. Device/resource loss or uncertainty requires `invalidate_update`, followed by full reconstruction. It is not a content reset.

This is not multi-renderer subscription/fan-out: one consumer cannot acknowledge on behalf of another independent incremental projection. Independent content readers are supported precisely because they do **not** share/consume this renderer baseline. Do not use `clear_screen_dirty` as an observer shortcut.

Existing tests: [core update replay](../../crates/harbor-terminal/src/core_tests.rs) (`core_update_replays_skipped_output_and_requires_explicit_acknowledgement`), [read isolation and generation tests](../../crates/harbor-terminal/src/read_model/tests.rs) (`independent_readers_do_not_acknowledge_damage_or_consume_parser_events`, `geometry_reset_replacement_and_buffer_transitions_have_distinct_lifetimes`), and [presentation projection tests](../../crates/harbor-terminal/src/read_model/presentation_tests.rs). These source links are not new test-run claims.
